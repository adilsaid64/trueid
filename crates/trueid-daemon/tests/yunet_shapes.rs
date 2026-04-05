//! YuNet ONNX I/O probe (`--ignored`).
#![cfg(test)]

use std::path::PathBuf;

use tract_onnx::prelude::*;

#[test]
#[ignore = "requires tests/fixtures/yunet.onnx"]
fn yunet_shapes() -> TractResult<()> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/yunet.onnx");
    let model = tract_onnx::onnx().model_for_path(&path)?.into_optimized()?;
    let ins = model.input_outlets()?;
    let outs = model.output_outlets()?;
    println!("input_outlets len={} {:?}", ins.len(), ins);
    println!("output_outlets len={} {:?}", outs.len(), outs);
    for o in ins.iter() {
        let fact = model.outlet_fact(*o)?;
        println!("IN {o:?}: {fact:?}");
    }
    for o in outs.iter() {
        let fact = model.outlet_fact(*o)?;
        println!("OUT {o:?}: {fact:?}");
    }
    Ok(())
}
