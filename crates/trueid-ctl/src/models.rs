use std::{fs, path::Path, process::Command};

const MODEL_DIR: &str = "/var/lib/trueid/models";

fn download(url: &str, dest: &str) -> Result<(), String> {
    if Path::new(dest).exists() {
        println!("Model already exists: {}", dest);
        return Ok(());
    }

    println!("Downloading {}...", dest);

    if Command::new("curl").arg("--version").output().is_ok() {
        let status = Command::new("curl")
            .args(["-L", "--fail", "-o", dest, url])
            .status()
            .map_err(|e| format!("failed to run curl: {e}"))?;

        if status.success() {
            return Ok(());
        }
    }

    if Command::new("wget").arg("--version").output().is_ok() {
        let status = Command::new("wget")
            .args(["-O", dest, url])
            .status()
            .map_err(|e| format!("failed to run wget: {e}"))?;

        if status.success() {
            return Ok(());
        }
    }

    Err("Neither curl nor wget is installed".into())
}

pub fn get_models() -> Result<(), String> {
    // Require root (since writing to /var/lib)
    if unsafe { libc::geteuid() } != 0 {
        return Err("Please run with sudo".into());
    }

    fs::create_dir_all(MODEL_DIR).map_err(|e| format!("failed to create model dir: {e}"))?;

    download(
        "https://github.com/opencv/opencv_zoo/raw/main/models/face_detection_yunet/face_detection_yunet_2023mar.onnx",
        &format!("{}/face_detection_yunet_2023mar.onnx", MODEL_DIR),
    )?;

    download(
        "https://huggingface.co/immich-app/buffalo_l/resolve/main/recognition/model.onnx",
        &format!("{}/face_embedding.onnx", MODEL_DIR),
    )?;

    println!("Models installed in {}", MODEL_DIR);

    Ok(())
}
