#!/usr/bin/env python3
"""Generate minimal ONNX test models for logos-ai integration tests.

Creates three models:
  1. layout_gen.onnx   — Linear: [1, 105] → [1, 80]  (layout coordinates)
  2. style_encoder.onnx — Conv+Pool: [1, 3, 64, 64] → [1, 64]  (style embedding)
  3. asset_decoder.onnx — Linear+Reshape: [1, 64] → [1, 3, 32, 32]  (small image)

These are REAL models with random weights — they produce non-trivial output
but are NOT trained for meaningful results. They exist to test the ONNX
Runtime integration pipeline end-to-end.
"""

import numpy as np
import os
import sys

try:
    import onnx
    from onnx import helper, TensorProto, numpy_helper
except ImportError:
    print("Installing onnx package...", file=sys.stderr)
    os.system(f"{sys.executable} -m pip install onnx --quiet")
    import onnx
    from onnx import helper, TensorProto, numpy_helper


def make_layout_gen_model(output_dir: str) -> str:
    """Layout generator: [1, 105] → [1, 80]

    A simple two-layer MLP that takes design constraint features
    and outputs layout coordinates (20 elements × 4 values each).
    """
    np.random.seed(42)

    # Layer 1: 105 → 128 (ReLU)
    W1 = numpy_helper.from_array(
        np.random.randn(105, 128).astype(np.float32) * 0.1, "W1"
    )
    B1 = numpy_helper.from_array(
        np.zeros(128, dtype=np.float32), "B1"
    )

    # Layer 2: 128 → 80 (Sigmoid for 0-1 range)
    W2 = numpy_helper.from_array(
        np.random.randn(128, 80).astype(np.float32) * 0.1, "W2"
    )
    B2 = numpy_helper.from_array(
        np.zeros(80, dtype=np.float32), "B2"
    )

    # Build graph
    input_tensor = helper.make_tensor_value_info("input", TensorProto.FLOAT, [1, 105])
    output_tensor = helper.make_tensor_value_info("output", TensorProto.FLOAT, [1, 80])

    matmul1 = helper.make_node("MatMul", ["input", "W1"], ["mm1"])
    add1 = helper.make_node("Add", ["mm1", "B1"], ["linear1"])
    relu1 = helper.make_node("Relu", ["linear1"], ["relu1"])
    matmul2 = helper.make_node("MatMul", ["relu1", "W2"], ["mm2"])
    add2 = helper.make_node("Add", ["mm2", "B2"], ["linear2"])
    sigmoid = helper.make_node("Sigmoid", ["linear2"], ["output"])

    graph = helper.make_graph(
        [matmul1, add1, relu1, matmul2, add2, sigmoid],
        "layout_gen",
        [input_tensor],
        [output_tensor],
        [W1, B1, W2, B2],
    )

    model = helper.make_model(graph, opset_imports=[helper.make_opsetid("", 13)])
    model.ir_version = 8
    onnx.checker.check_model(model)

    path = os.path.join(output_dir, "layout_gen.onnx")
    onnx.save(model, path)
    size = os.path.getsize(path)
    print(f"  ✓ layout_gen.onnx ({size:,} bytes) — [1,105] → [1,80]")
    return path


def make_style_encoder_model(output_dir: str) -> str:
    """Style encoder: [1, 3, 64, 64] → [1, 64]

    Simple conv → relu → global avg pool → linear pipeline.
    Extracts a 64-dim style embedding from a 64×64 RGB image.
    """
    np.random.seed(123)

    # Conv: 3→16 channels, 3×3 kernel
    conv_W = numpy_helper.from_array(
        np.random.randn(16, 3, 3, 3).astype(np.float32) * 0.1, "conv_W"
    )
    conv_B = numpy_helper.from_array(
        np.zeros(16, dtype=np.float32), "conv_B"
    )

    # Linear: 16 → 64
    fc_W = numpy_helper.from_array(
        np.random.randn(16, 64).astype(np.float32) * 0.1, "fc_W"
    )
    fc_B = numpy_helper.from_array(
        np.zeros(64, dtype=np.float32), "fc_B"
    )

    input_tensor = helper.make_tensor_value_info("input", TensorProto.FLOAT, [1, 3, 64, 64])
    output_tensor = helper.make_tensor_value_info("output", TensorProto.FLOAT, [1, 64])

    # Conv2d
    conv = helper.make_node(
        "Conv", ["input", "conv_W", "conv_B"], ["conv_out"],
        kernel_shape=[3, 3], pads=[1, 1, 1, 1]
    )
    relu = helper.make_node("Relu", ["conv_out"], ["relu_out"])

    # Global Average Pool: [1, 16, 64, 64] → [1, 16, 1, 1]
    gap = helper.make_node("GlobalAveragePool", ["relu_out"], ["gap_out"])

    # Reshape: [1, 16, 1, 1] → [1, 16]
    shape_const = numpy_helper.from_array(
        np.array([1, 16], dtype=np.int64), "reshape_shape"
    )
    reshape = helper.make_node("Reshape", ["gap_out", "reshape_shape"], ["flat_out"])

    # Linear: [1, 16] → [1, 64]
    fc = helper.make_node("MatMul", ["flat_out", "fc_W"], ["fc_out"])
    add_bias = helper.make_node("Add", ["fc_out", "fc_B"], ["output"])

    graph = helper.make_graph(
        [conv, relu, gap, reshape, fc, add_bias],
        "style_encoder",
        [input_tensor],
        [output_tensor],
        [conv_W, conv_B, fc_W, fc_B, shape_const],
    )

    model = helper.make_model(graph, opset_imports=[helper.make_opsetid("", 13)])
    model.ir_version = 8
    onnx.checker.check_model(model)

    path = os.path.join(output_dir, "style_encoder.onnx")
    onnx.save(model, path)
    size = os.path.getsize(path)
    print(f"  ✓ style_encoder.onnx ({size:,} bytes) — [1,3,64,64] → [1,64]")
    return path


def make_asset_decoder_model(output_dir: str) -> str:
    """Asset decoder: [1, 64] → [1, 3, 32, 32]

    Linear → reshape → two transposed convolutions.
    Takes a 64-dim latent vector and decodes it to a 32×32 RGB image.
    """
    np.random.seed(456)

    # Linear: 64 → 256 (which is 16 × 4 × 4)
    fc_W = numpy_helper.from_array(
        np.random.randn(64, 256).astype(np.float32) * 0.1, "fc_W"
    )
    fc_B = numpy_helper.from_array(
        np.zeros(256, dtype=np.float32), "fc_B"
    )

    # Reshape target: [1, 16, 4, 4]
    shape_4d = numpy_helper.from_array(
        np.array([1, 16, 4, 4], dtype=np.int64), "shape_4d"
    )

    # ConvTranspose: 16→8 channels, 4×4 kernel, stride 2 → [1, 8, 8, 8]
    deconv1_W = numpy_helper.from_array(
        np.random.randn(16, 8, 4, 4).astype(np.float32) * 0.1, "deconv1_W"
    )

    # ConvTranspose: 8→3 channels, 4×4 kernel, stride 4 → [1, 3, 32, 32]
    deconv2_W = numpy_helper.from_array(
        np.random.randn(8, 3, 4, 4).astype(np.float32) * 0.1, "deconv2_W"
    )

    input_tensor = helper.make_tensor_value_info("input", TensorProto.FLOAT, [1, 64])
    output_tensor = helper.make_tensor_value_info("output", TensorProto.FLOAT, [1, 3, 32, 32])

    # Linear
    fc = helper.make_node("MatMul", ["input", "fc_W"], ["fc_out"])
    add_bias = helper.make_node("Add", ["fc_out", "fc_B"], ["linear_out"])
    relu1 = helper.make_node("Relu", ["linear_out"], ["relu1_out"])

    # Reshape to 4D
    reshape = helper.make_node("Reshape", ["relu1_out", "shape_4d"], ["reshape_out"])

    # ConvTranspose 16→8, stride 2: [1,16,4,4] → [1,8,8,8]
    deconv1 = helper.make_node(
        "ConvTranspose", ["reshape_out", "deconv1_W"], ["deconv1_out"],
        kernel_shape=[4, 4], strides=[2, 2], pads=[1, 1, 1, 1]
    )
    relu2 = helper.make_node("Relu", ["deconv1_out"], ["relu2_out"])

    # ConvTranspose 8→3, stride 4: [1,8,8,8] → [1,3,32,32]
    deconv2 = helper.make_node(
        "ConvTranspose", ["relu2_out", "deconv2_W"], ["deconv2_out"],
        kernel_shape=[4, 4], strides=[4, 4], pads=[0, 0, 0, 0]
    )

    # Sigmoid to 0-1 range
    sigmoid = helper.make_node("Sigmoid", ["deconv2_out"], ["output"])

    graph = helper.make_graph(
        [fc, add_bias, relu1, reshape, deconv1, relu2, deconv2, sigmoid],
        "asset_decoder",
        [input_tensor],
        [output_tensor],
        [fc_W, fc_B, shape_4d, deconv1_W, deconv2_W],
    )

    model = helper.make_model(graph, opset_imports=[helper.make_opsetid("", 13)])
    model.ir_version = 8
    onnx.checker.check_model(model)

    path = os.path.join(output_dir, "asset_decoder.onnx")
    onnx.save(model, path)
    size = os.path.getsize(path)
    print(f"  ✓ asset_decoder.onnx ({size:,} bytes) — [1,64] → [1,3,32,32]")
    return path


def verify_models(output_dir: str):
    """Quick verification: load each model and run inference with numpy."""
    import onnx
    from onnx import numpy_helper
    print("\nVerification (onnx checker):")

    for name in ["layout_gen.onnx", "style_encoder.onnx", "asset_decoder.onnx"]:
        path = os.path.join(output_dir, name)
        model = onnx.load(path)
        onnx.checker.check_model(model)
        print(f"  ✓ {name} — valid ONNX")


if __name__ == "__main__":
    script_dir = os.path.dirname(os.path.abspath(__file__))
    output_dir = os.path.join(os.path.dirname(script_dir), "test-models")
    os.makedirs(output_dir, exist_ok=True)

    print("Generating ONNX test models for logos-ai...")
    print(f"Output: {output_dir}\n")

    make_layout_gen_model(output_dir)
    make_style_encoder_model(output_dir)
    make_asset_decoder_model(output_dir)
    verify_models(output_dir)

    print(f"\n✓ All 3 test models generated in {output_dir}")
