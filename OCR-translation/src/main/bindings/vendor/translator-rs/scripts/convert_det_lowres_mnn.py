#!/usr/bin/env python3
"""Fold the DBNet head deconvs of the PP-OCRv5 det model so the probability
map comes out at 1/2 or 1/4 of the input resolution instead of full res.

Both deconvs are 2x2 stride-2 unpadded, so each folds exactly into a 1x1
conv with spatially-averaged weights — equivalent to average-pooling the
full-res logit map (the 1/4 variant is approximate only through the BN+ReLU
between the two deconvs). Sigmoid stays in-graph on the small map.
extract_boxes handles the smaller mask via the stride derived from the
output shape.
"""

import argparse
import subprocess
from pathlib import Path

import numpy as np
import onnx
from onnx import helper, numpy_helper

VARIANTS = {
    "half": ["ConvTranspose.2"],
    "quarter": ["ConvTranspose.2", "ConvTranspose.0"],
}


def fold_deconv_weight(w: np.ndarray) -> np.ndarray:
    # ConvTranspose [Cin, Cout, 2, 2] -> Conv [Cout, Cin, 1, 1], spatial mean.
    return w.mean(axis=(2, 3)).T[:, :, None, None].astype(np.float32)


def replace_deconv(graph: onnx.GraphProto, node_name: str, new_w_name: str) -> None:
    inits = {i.name: i for i in graph.initializer}
    for idx, node in enumerate(graph.node):
        if node.name != node_name:
            continue
        w = numpy_helper.to_array(inits[node.input[1]])
        graph.initializer.append(numpy_helper.from_array(fold_deconv_weight(w), new_w_name))
        conv = helper.make_node(
            "Conv",
            [node.input[0], new_w_name],
            list(node.output),
            name=node_name + "_folded",
            kernel_shape=[1, 1],
            strides=[1, 1],
            pads=[0, 0, 0, 0],
        )
        graph.node.remove(node)
        graph.node.insert(idx, conv)
        return
    raise KeyError(f"node {node_name} not found")


def clear_output_dims(graph: onnx.GraphProto) -> None:
    for output in graph.output:
        for dim in output.type.tensor_type.shape.dim:
            dim.Clear()
            dim.dim_param = "dyn"


def main() -> None:
    repo_root = Path(__file__).resolve().parent.parent
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--src-onnx",
        type=Path,
        default=Path.home() / "AndroidStudioProjects/OCR/onnx_out/PP-OCRv5_mobile_det.onnx",
    )
    parser.add_argument("--out-dir", type=Path, default=Path("/tmp/det-variants"))
    parser.add_argument(
        "--mnnconvert",
        type=Path,
        default=repo_root.parent / "mnn-sys/3rd_party/MNN/build-convert/MNNConvert",
    )
    parser.add_argument("--quant-bits", type=int, default=8)
    args = parser.parse_args()

    args.out_dir.mkdir(parents=True, exist_ok=True)
    for name, deconvs in VARIANTS.items():
        model = onnx.load(args.src_onnx)
        for deconv in deconvs:
            replace_deconv(model.graph, deconv, f"folded_{deconv}")
        clear_output_dims(model.graph)
        onnx_path = args.out_dir / f"det_{name}.onnx"
        onnx.save(model, onnx_path)

        mnn_path = args.out_dir / f"det_{name}_int8.mnn"
        subprocess.run(
            [
                args.mnnconvert,
                "-f", "ONNX",
                "--modelFile", onnx_path,
                "--MNNModel", mnn_path,
                "--bizCode", "biz",
                "--weightQuantBits", str(args.quant_bits),
            ],
            check=True,
            capture_output=True,
        )
        print(mnn_path)


if __name__ == "__main__":
    main()
