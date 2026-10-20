"""Patch Kokoro fp32 ONNX into the stable MNN conversion source.

The output is `kokoro-v1.0.patched.i32.onnx` by default. It applies the
Resize/Round cleanup and normalizes the token-id graph input to
`input_ids: INT32` so MNN's Gather path matches ORT. Kokoro ONNX exports have
used both `tokens` and `input_ids`; this script accepts either source name.

For each Resize node listed in TARGETS we:
  - clear the `scales` input (index 2)
  - feed an explicit `sizes` input (index 3) computed from the input's runtime
    Shape, multiplied/divided by the integer factor.

This removes any rounding-mode dependence between ONNX runtimes.
"""

import argparse
from pathlib import Path

import numpy as np
import onnx
import onnx_graphsurgeon as gs


# (node name, integer factor; positive=upsample by N, negative=downsample by |N|)
TARGETS = [
    ("/encoder/F0.1/upsample/Resize", 2),
    ("/encoder/N.1/upsample/Resize", 2),
    ("/decoder/decoder/generator/f0_upsamp/Resize", 300),
    ("/decoder/decoder/generator/m_source/l_sin_gen/Resize", -300),
    ("/decoder/decoder/generator/m_source/l_sin_gen/Resize_1", 300),
    ("/decoder/decoder/decode.3/upsample/Resize", 2),
]


def replace_round_with_floor_half_up(graph: gs.Graph) -> None:
    """ONNX Round = banker's rounding (round half to even); MNN's Round
    implementation rounds half away from zero. They disagree on x.5 values.
    Replace Round with Floor(x + 0.5), which both runtimes evaluate identically.
    """
    for node in list(graph.nodes):
        if node.op != "Round":
            continue
        x = node.inputs[0]
        unique = node.name.replace("/", "_").lstrip("_")
        half = gs.Constant(f"{unique}/half", values=np.array(0.5, dtype=np.float32))
        added = gs.Variable(f"{unique}/x_plus_half", dtype=np.float32)
        add_node = gs.Node(op="Add", inputs=[x, half], outputs=[added])
        # Rewire the downstream output through Floor instead of Round.
        original_out = node.outputs[0]
        floor_node = gs.Node(op="Floor", name=f"{node.name}_as_floor", inputs=[added], outputs=[original_out])
        node.outputs = []  # detach so cleanup() removes it
        graph.nodes.extend([add_node, floor_node])
        print(f"replaced {node.name} (Round) with Add+Floor")


def normalize_token_input_to_int32(graph: gs.Graph) -> None:
    """MNN 3.5 miscomputes Gather when the ONNX indices input is INT64.
    Kokoro token ids fit in INT32, and ONNX Gather accepts INT32 indices, so
    narrowing this model input keeps ORT semantics while avoiding the MNN bug.
    Normalize the public name to `input_ids` because the app integration uses
    named MNN inputs.
    """
    for inp in graph.inputs:
        if inp.name in ("input_ids", "tokens"):
            old_name = inp.name
            inp.name = "input_ids"
            inp.dtype = np.int32
            print(f"changed token input {old_name} to input_ids INT32")
            return
    names = ", ".join(inp.name for inp in graph.inputs)
    raise SystemExit(f"token input not found; expected input_ids or tokens, got: {names}")


def patch(input_path: Path, output_path: Path) -> None:
    graph = gs.import_onnx(onnx.load(str(input_path)))
    node_by_name = {n.name: n for n in graph.nodes}

    normalize_token_input_to_int32(graph)
    replace_round_with_floor_half_up(graph)

    for name, factor in TARGETS:
        node = node_by_name.get(name)
        if node is None:
            raise SystemExit(f"node not found: {name}")
        if node.op != "Resize":
            raise SystemExit(f"node {name} is not Resize: {node.op}")
        if len(node.inputs) < 3:
            raise SystemExit(f"node {name} has fewer than 3 inputs")

        src = node.inputs[0]
        unique = name.replace("/", "_").lstrip("_")

        # Shape(src) -> [N, C, T]
        shape_out = gs.Variable(f"{unique}/shape_out", dtype=np.int64)
        shape_node = gs.Node(op="Shape", inputs=[src], outputs=[shape_out])

        # Slice the time dim: shape[2:3]
        starts = gs.Constant(f"{unique}/slice_starts", values=np.array([2], dtype=np.int64))
        ends = gs.Constant(f"{unique}/slice_ends", values=np.array([3], dtype=np.int64))
        axes = gs.Constant(f"{unique}/slice_axes", values=np.array([0], dtype=np.int64))
        t_dim = gs.Variable(f"{unique}/t_dim", dtype=np.int64)
        slice_node = gs.Node(op="Slice", inputs=[shape_out, starts, ends, axes], outputs=[t_dim])

        if factor > 0:
            scale_const = gs.Constant(f"{unique}/factor", values=np.array([factor], dtype=np.int64))
            t_new = gs.Variable(f"{unique}/t_new", dtype=np.int64)
            arith = gs.Node(op="Mul", inputs=[t_dim, scale_const], outputs=[t_new])
        else:
            scale_const = gs.Constant(f"{unique}/factor", values=np.array([-factor], dtype=np.int64))
            t_new = gs.Variable(f"{unique}/t_new", dtype=np.int64)
            arith = gs.Node(op="Div", inputs=[t_dim, scale_const], outputs=[t_new])

        # Take N, C dims (shape[0:2])
        nc_starts = gs.Constant(f"{unique}/nc_starts", values=np.array([0], dtype=np.int64))
        nc_ends = gs.Constant(f"{unique}/nc_ends", values=np.array([2], dtype=np.int64))
        nc_axes = gs.Constant(f"{unique}/nc_axes", values=np.array([0], dtype=np.int64))
        nc_dim = gs.Variable(f"{unique}/nc_dim", dtype=np.int64)
        nc_slice = gs.Node(op="Slice", inputs=[shape_out, nc_starts, nc_ends, nc_axes], outputs=[nc_dim])

        sizes_out = gs.Variable(f"{unique}/sizes", dtype=np.int64)
        concat_node = gs.Node(op="Concat", attrs={"axis": 0}, inputs=[nc_dim, t_new], outputs=[sizes_out])

        graph.nodes.extend([shape_node, slice_node, arith, nc_slice, concat_node])

        # Resize signature: X, roi (opt), scales (opt), sizes (opt).
        # Clear scales (index 2), set sizes (index 3).
        empty_in = gs.Variable.empty()
        node.inputs[2] = empty_in
        if len(node.inputs) >= 4:
            node.inputs[3] = sizes_out
        else:
            node.inputs.append(sizes_out)

        print(f"patched {name}: factor={factor}")

    graph.cleanup().toposort()
    onnx.save(gs.export_onnx(graph), str(output_path))
    print(f"wrote {output_path}")


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--input", default="kokoro-v1.0.onnx")
    ap.add_argument("--output", default="kokoro-v1.0.patched.i32.onnx")
    args = ap.parse_args()
    patch(Path(args.input), Path(args.output))


if __name__ == "__main__":
    main()
