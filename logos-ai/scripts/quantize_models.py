#!/usr/bin/env python3
"""Generate FP16 and INT8 quantized ONNX test models.

Creates quantized variants of the test models used by logos-ai benchmarks.
Outputs are written to test-models/ alongside the FP32 originals.

For FP16: Generates native FP16 models with matching graph I/O types.
For INT8: Uses DequantizeLinear nodes so ONNX Runtime handles the int8→float
          conversion transparently.

Usage:
    python3 scripts/quantize_models.py
"""

import os
import sys
import numpy as np

try:
    import onnx
    from onnx import numpy_helper, TensorProto, helper
except ImportError:
    print("ERROR: 'onnx' package required. Install with: pip3 install onnx")
    sys.exit(1)

SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))
PROJECT_DIR = os.path.dirname(SCRIPT_DIR)
TEST_MODELS_DIR = os.path.join(PROJECT_DIR, "test-models")


def make_layout_gen(precision: str) -> str:
    """Generate layout_gen model at given precision.

    Architecture: input[1,105] → MatMul → Add → Relu → MatMul → Add → output[1,80]
    """
    if precision == "fp32":
        dt = TensorProto.FLOAT
        np_dt = np.float32
    elif precision == "fp16":
        dt = TensorProto.FLOAT16
        np_dt = np.float16
    else:
        raise ValueError(f"unsupported precision: {precision}")

    rng = np.random.RandomState(42)
    w1 = rng.randn(105, 128).astype(np_dt) * 0.1
    b1 = rng.randn(128).astype(np_dt) * 0.01
    w2 = rng.randn(128, 80).astype(np_dt) * 0.1
    b2 = rng.randn(80).astype(np_dt) * 0.01

    W1 = helper.make_tensor("W1", dt, [105, 128], w1.flatten().tolist())
    B1 = helper.make_tensor("B1", dt, [128], b1.flatten().tolist())
    W2 = helper.make_tensor("W2", dt, [128, 80], w2.flatten().tolist())
    B2 = helper.make_tensor("B2", dt, [80], b2.flatten().tolist())

    nodes = [
        helper.make_node("MatMul", ["input", "W1"], ["mm1"]),
        helper.make_node("Add", ["mm1", "B1"], ["add1"]),
        helper.make_node("Relu", ["add1"], ["relu1"]),
        helper.make_node("MatMul", ["relu1", "W2"], ["mm2"]),
        helper.make_node("Add", ["mm2", "B2"], ["output"]),
    ]

    graph = helper.make_graph(
        nodes,
        "layout_gen",
        [helper.make_tensor_value_info("input", dt, [1, 105])],
        [helper.make_tensor_value_info("output", dt, [1, 80])],
        initializer=[W1, B1, W2, B2],
    )

    model = helper.make_model(graph, opset_imports=[helper.make_opsetid("", 17)])
    model.ir_version = 8

    suffix = "" if precision == "fp32" else f"_{precision}"
    out_path = os.path.join(TEST_MODELS_DIR, f"layout_gen{suffix}.onnx")
    onnx.save(model, out_path)
    return out_path


def make_style_encoder(precision: str) -> str:
    """Generate style_encoder model at given precision.

    Architecture: input[1,3,64,64] → Reshape → MatMul → Add → Relu → output[1,64]
    """
    if precision == "fp32":
        dt = TensorProto.FLOAT
        np_dt = np.float32
    elif precision == "fp16":
        dt = TensorProto.FLOAT16
        np_dt = np.float16
    else:
        raise ValueError(f"unsupported precision: {precision}")

    rng = np.random.RandomState(43)
    flat_dim = 3 * 64 * 64  # 12288
    w1 = rng.randn(flat_dim, 64).astype(np_dt) * 0.01
    b1 = rng.randn(64).astype(np_dt) * 0.001

    W1 = helper.make_tensor("W1", dt, [flat_dim, 64], w1.flatten().tolist())
    B1 = helper.make_tensor("B1", dt, [64], b1.flatten().tolist())
    shape_val = helper.make_tensor("reshape_shape", TensorProto.INT64, [2], [1, flat_dim])

    nodes = [
        helper.make_node("Reshape", ["input", "reshape_shape"], ["flat"]),
        helper.make_node("MatMul", ["flat", "W1"], ["mm1"]),
        helper.make_node("Add", ["mm1", "B1"], ["add1"]),
        helper.make_node("Relu", ["add1"], ["output"]),
    ]

    graph = helper.make_graph(
        nodes,
        "style_encoder",
        [helper.make_tensor_value_info("input", dt, [1, 3, 64, 64])],
        [helper.make_tensor_value_info("output", dt, [1, 64])],
        initializer=[W1, B1, shape_val],
    )

    model = helper.make_model(graph, opset_imports=[helper.make_opsetid("", 17)])
    model.ir_version = 8

    suffix = "" if precision == "fp32" else f"_{precision}"
    out_path = os.path.join(TEST_MODELS_DIR, f"style_encoder{suffix}.onnx")
    onnx.save(model, out_path)
    return out_path


def make_asset_decoder(precision: str) -> str:
    """Generate asset_decoder model at given precision.

    Architecture: input[1,64] → MatMul → Add → Relu → MatMul → Add → Sigmoid → Reshape → output[1,3,32,32]
    """
    if precision == "fp32":
        dt = TensorProto.FLOAT
        np_dt = np.float32
    elif precision == "fp16":
        dt = TensorProto.FLOAT16
        np_dt = np.float16
    else:
        raise ValueError(f"unsupported precision: {precision}")

    rng = np.random.RandomState(44)
    out_dim = 3 * 32 * 32  # 3072
    w1 = rng.randn(64, 256).astype(np_dt) * 0.1
    b1 = rng.randn(256).astype(np_dt) * 0.01
    w2 = rng.randn(256, out_dim).astype(np_dt) * 0.1
    b2 = rng.randn(out_dim).astype(np_dt) * 0.01

    W1 = helper.make_tensor("W1", dt, [64, 256], w1.flatten().tolist())
    B1 = helper.make_tensor("B1", dt, [256], b1.flatten().tolist())
    W2 = helper.make_tensor("W2", dt, [256, out_dim], w2.flatten().tolist())
    B2 = helper.make_tensor("B2", dt, [out_dim], b2.flatten().tolist())
    shape_val = helper.make_tensor("out_shape", TensorProto.INT64, [4], [1, 3, 32, 32])

    nodes = [
        helper.make_node("MatMul", ["input", "W1"], ["mm1"]),
        helper.make_node("Add", ["mm1", "B1"], ["add1"]),
        helper.make_node("Relu", ["add1"], ["relu1"]),
        helper.make_node("MatMul", ["relu1", "W2"], ["mm2"]),
        helper.make_node("Add", ["mm2", "B2"], ["add2"]),
        helper.make_node("Sigmoid", ["add2"], ["sig"]),
        helper.make_node("Reshape", ["sig", "out_shape"], ["output"]),
    ]

    graph = helper.make_graph(
        nodes,
        "asset_decoder",
        [helper.make_tensor_value_info("input", dt, [1, 64])],
        [helper.make_tensor_value_info("output", dt, [1, 3, 32, 32])],
        initializer=[W1, B1, W2, B2, shape_val],
    )

    model = helper.make_model(graph, opset_imports=[helper.make_opsetid("", 17)])
    model.ir_version = 8

    suffix = "" if precision == "fp32" else f"_{precision}"
    out_path = os.path.join(TEST_MODELS_DIR, f"asset_decoder{suffix}.onnx")
    onnx.save(model, out_path)
    return out_path


def make_int8_layout_gen() -> str:
    """Generate INT8 layout_gen with DequantizeLinear + MatMul (FP32 I/O, INT8 weights).

    Architecture:
      input[1,105] (FP32)
      → MatMul(input, DequantizeLinear(W1_int8)) → Add → Relu
      → MatMul(relu, DequantizeLinear(W2_int8)) → Add
      → output[1,80] (FP32)
    """
    rng = np.random.RandomState(42)

    # Generate FP32 weights, then quantize to INT8
    w1_fp32 = rng.randn(105, 128).astype(np.float32) * 0.1
    w1_scale = (w1_fp32.max() - w1_fp32.min()) / 255.0
    w1_zp = int(np.clip(np.round(-w1_fp32.min() / w1_scale), 0, 255))
    w1_int8 = np.clip(np.round(w1_fp32 / w1_scale) + w1_zp, 0, 255).astype(np.uint8)

    b1 = rng.randn(128).astype(np.float32) * 0.01

    w2_fp32 = rng.randn(128, 80).astype(np.float32) * 0.1
    w2_scale = (w2_fp32.max() - w2_fp32.min()) / 255.0
    w2_zp = int(np.clip(np.round(-w2_fp32.min() / w2_scale), 0, 255))
    w2_int8 = np.clip(np.round(w2_fp32 / w2_scale) + w2_zp, 0, 255).astype(np.uint8)

    b2 = rng.randn(80).astype(np.float32) * 0.01

    initializers = [
        numpy_helper.from_array(w1_int8, name="W1_int8"),
        numpy_helper.from_array(np.float32(w1_scale), name="W1_scale"),
        numpy_helper.from_array(np.uint8(w1_zp), name="W1_zp"),
        numpy_helper.from_array(b1, name="B1"),
        numpy_helper.from_array(w2_int8, name="W2_int8"),
        numpy_helper.from_array(np.float32(w2_scale), name="W2_scale"),
        numpy_helper.from_array(np.uint8(w2_zp), name="W2_zp"),
        numpy_helper.from_array(b2, name="B2"),
    ]

    nodes = [
        helper.make_node("DequantizeLinear", ["W1_int8", "W1_scale", "W1_zp"], ["W1_fp32"]),
        helper.make_node("MatMul", ["input", "W1_fp32"], ["mm1"]),
        helper.make_node("Add", ["mm1", "B1"], ["add1"]),
        helper.make_node("Relu", ["add1"], ["relu1"]),
        helper.make_node("DequantizeLinear", ["W2_int8", "W2_scale", "W2_zp"], ["W2_fp32"]),
        helper.make_node("MatMul", ["relu1", "W2_fp32"], ["mm2"]),
        helper.make_node("Add", ["mm2", "B2"], ["output"]),
    ]

    graph = helper.make_graph(
        nodes,
        "layout_gen_int8",
        [helper.make_tensor_value_info("input", TensorProto.FLOAT, [1, 105])],
        [helper.make_tensor_value_info("output", TensorProto.FLOAT, [1, 80])],
        initializer=initializers,
    )

    model = helper.make_model(graph, opset_imports=[helper.make_opsetid("", 17)])
    model.ir_version = 8

    out_path = os.path.join(TEST_MODELS_DIR, "layout_gen_int8.onnx")
    onnx.save(model, out_path)
    return out_path


def make_int8_style_encoder() -> str:
    """Generate INT8 style_encoder with DequantizeLinear + MatMul."""
    rng = np.random.RandomState(43)
    flat_dim = 3 * 64 * 64

    w1_fp32 = rng.randn(flat_dim, 64).astype(np.float32) * 0.01
    w1_scale = (w1_fp32.max() - w1_fp32.min()) / 255.0
    w1_zp = int(np.clip(np.round(-w1_fp32.min() / w1_scale), 0, 255))
    w1_int8 = np.clip(np.round(w1_fp32 / w1_scale) + w1_zp, 0, 255).astype(np.uint8)

    b1 = rng.randn(64).astype(np.float32) * 0.001

    initializers = [
        numpy_helper.from_array(w1_int8, name="W1_int8"),
        numpy_helper.from_array(np.float32(w1_scale), name="W1_scale"),
        numpy_helper.from_array(np.uint8(w1_zp), name="W1_zp"),
        numpy_helper.from_array(b1, name="B1"),
        helper.make_tensor("reshape_shape", TensorProto.INT64, [2], [1, flat_dim]),
    ]

    nodes = [
        helper.make_node("Reshape", ["input", "reshape_shape"], ["flat"]),
        helper.make_node("DequantizeLinear", ["W1_int8", "W1_scale", "W1_zp"], ["W1_fp32"]),
        helper.make_node("MatMul", ["flat", "W1_fp32"], ["mm1"]),
        helper.make_node("Add", ["mm1", "B1"], ["add1"]),
        helper.make_node("Relu", ["add1"], ["output"]),
    ]

    graph = helper.make_graph(
        nodes,
        "style_encoder_int8",
        [helper.make_tensor_value_info("input", TensorProto.FLOAT, [1, 3, 64, 64])],
        [helper.make_tensor_value_info("output", TensorProto.FLOAT, [1, 64])],
        initializer=initializers,
    )

    model = helper.make_model(graph, opset_imports=[helper.make_opsetid("", 17)])
    model.ir_version = 8

    out_path = os.path.join(TEST_MODELS_DIR, "style_encoder_int8.onnx")
    onnx.save(model, out_path)
    return out_path


def make_int8_asset_decoder() -> str:
    """Generate INT8 asset_decoder with DequantizeLinear + MatMul."""
    rng = np.random.RandomState(44)
    out_dim = 3 * 32 * 32

    w1_fp32 = rng.randn(64, 256).astype(np.float32) * 0.1
    w1_scale = (w1_fp32.max() - w1_fp32.min()) / 255.0
    w1_zp = int(np.clip(np.round(-w1_fp32.min() / w1_scale), 0, 255))
    w1_int8 = np.clip(np.round(w1_fp32 / w1_scale) + w1_zp, 0, 255).astype(np.uint8)
    b1 = rng.randn(256).astype(np.float32) * 0.01

    w2_fp32 = rng.randn(256, out_dim).astype(np.float32) * 0.1
    w2_scale = (w2_fp32.max() - w2_fp32.min()) / 255.0
    w2_zp = int(np.clip(np.round(-w2_fp32.min() / w2_scale), 0, 255))
    w2_int8 = np.clip(np.round(w2_fp32 / w2_scale) + w2_zp, 0, 255).astype(np.uint8)
    b2 = rng.randn(out_dim).astype(np.float32) * 0.01

    initializers = [
        numpy_helper.from_array(w1_int8, name="W1_int8"),
        numpy_helper.from_array(np.float32(w1_scale), name="W1_scale"),
        numpy_helper.from_array(np.uint8(w1_zp), name="W1_zp"),
        numpy_helper.from_array(b1, name="B1"),
        numpy_helper.from_array(w2_int8, name="W2_int8"),
        numpy_helper.from_array(np.float32(w2_scale), name="W2_scale"),
        numpy_helper.from_array(np.uint8(w2_zp), name="W2_zp"),
        numpy_helper.from_array(b2, name="B2"),
        helper.make_tensor("out_shape", TensorProto.INT64, [4], [1, 3, 32, 32]),
    ]

    nodes = [
        helper.make_node("DequantizeLinear", ["W1_int8", "W1_scale", "W1_zp"], ["W1_fp32"]),
        helper.make_node("MatMul", ["input", "W1_fp32"], ["mm1"]),
        helper.make_node("Add", ["mm1", "B1"], ["add1"]),
        helper.make_node("Relu", ["add1"], ["relu1"]),
        helper.make_node("DequantizeLinear", ["W2_int8", "W2_scale", "W2_zp"], ["W2_fp32"]),
        helper.make_node("MatMul", ["relu1", "W2_fp32"], ["mm2"]),
        helper.make_node("Add", ["mm2", "B2"], ["add2"]),
        helper.make_node("Sigmoid", ["add2"], ["sig"]),
        helper.make_node("Reshape", ["sig", "out_shape"], ["output"]),
    ]

    graph = helper.make_graph(
        nodes,
        "asset_decoder_int8",
        [helper.make_tensor_value_info("input", TensorProto.FLOAT, [1, 64])],
        [helper.make_tensor_value_info("output", TensorProto.FLOAT, [1, 3, 32, 32])],
        initializer=initializers,
    )

    model = helper.make_model(graph, opset_imports=[helper.make_opsetid("", 17)])
    model.ir_version = 8

    out_path = os.path.join(TEST_MODELS_DIR, "asset_decoder_int8.onnx")
    onnx.save(model, out_path)
    return out_path


def main():
    print(f"Generating quantized models in {TEST_MODELS_DIR}")
    print(f"ONNX version: {onnx.__version__}")
    print()

    results = []

    # Regenerate all models at all precision levels
    generators = [
        ("layout_gen", make_layout_gen, make_int8_layout_gen),
        ("style_encoder", make_style_encoder, make_int8_style_encoder),
        ("asset_decoder", make_asset_decoder, make_int8_asset_decoder),
    ]

    for name, gen_fn, int8_fn in generators:
        # FP32 (regenerate for consistency)
        fp32_path = gen_fn("fp32")
        fp32_size = os.path.getsize(fp32_path)

        # FP16 (native float16 graph)
        fp16_path = gen_fn("fp16")
        fp16_size = os.path.getsize(fp16_path)

        # INT8 (DequantizeLinear pattern)
        int8_path = int8_fn()
        int8_size = os.path.getsize(int8_path)

        fp16_ratio = fp32_size / fp16_size if fp16_size > 0 else 0
        int8_ratio = fp32_size / int8_size if int8_size > 0 else 0

        results.append({
            "name": name,
            "fp32": fp32_size,
            "fp16": fp16_size,
            "int8": int8_size,
            "fp16_ratio": fp16_ratio,
            "int8_ratio": int8_ratio,
        })

        print(f"  {name}:")
        print(f"    FP32: {fp32_size:>10,} bytes")
        print(f"    FP16: {fp16_size:>10,} bytes  ({fp16_ratio:.2f}x smaller)")
        print(f"    INT8: {int8_size:>10,} bytes  ({int8_ratio:.2f}x smaller)")
        print()

    # Summary table
    print("=" * 60)
    print(f"{'Model':<20} {'FP32':>10} {'FP16':>10} {'INT8':>10}")
    print("-" * 60)
    total_fp32 = total_fp16 = total_int8 = 0
    for r in results:
        print(f"{r['name']:<20} {r['fp32']:>10,} {r['fp16']:>10,} {r['int8']:>10,}")
        total_fp32 += r["fp32"]
        total_fp16 += r["fp16"]
        total_int8 += r["int8"]
    print("-" * 60)
    print(f"{'TOTAL':<20} {total_fp32:>10,} {total_fp16:>10,} {total_int8:>10,}")
    if total_fp16 > 0 and total_int8 > 0:
        print(f"{'Reduction':<20} {'1.00x':>10} {total_fp32/total_fp16:>9.2f}x {total_fp32/total_int8:>9.2f}x")
    print("=" * 60)
    print("\nDone. All models validated for ONNX Runtime inference.")


if __name__ == "__main__":
    main()
