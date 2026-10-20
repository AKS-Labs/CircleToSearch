"""Export a checkpoint to ONNX with dynamic width, then verify against torch.

python export_onnx.py --ckpt ckpt/ink-latest.pt --out ink.onnx
MNN conversion afterwards (same toolchain as the PP-OCR models):
  MNNConvert -f ONNX --modelFile ink.onnx --MNNModel ink.mnn --bizCode ink
"""

import argparse

import numpy as np
import torch

from model import InkUNet


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--ckpt", required=True)
    ap.add_argument("--out", required=True)
    args = ap.parse_args()

    state = torch.load(args.ckpt, map_location="cpu")
    model = InkUNet(base=state["base"], levels=state["levels"],
                    bold_from=state.get("bold_from", 1), bold_head=state.get("bold_head", "dilated"))
    model.load_state_dict(state["model"])
    model.eval()

    dummy = torch.zeros(1, 3, 48, 320)
    torch.onnx.export(
        model,
        dummy,
        args.out,
        input_names=["strip"],
        output_names=["ink_logits"],
        dynamic_axes={"strip": {3: "width"}, "ink_logits": {3: "width"}},
        opset_version=17,
    )

    import onnxruntime as ort  # noqa: PLC0415

    sess = ort.InferenceSession(args.out, providers=["CPUExecutionProvider"])
    for width in (160, 320, 504):
        x = torch.rand(1, 3, 48, width)
        with torch.no_grad():
            ref = model(x).numpy()
        got = sess.run(None, {"strip": x.numpy()})[0]
        diff = float(np.abs(ref - got).max())
        print(f"width {width}: max |torch - onnx| = {diff:.2e}")
        assert diff < 1e-3, "onnx export mismatch"
    print(f"ok -> {args.out}")


if __name__ == "__main__":
    main()
