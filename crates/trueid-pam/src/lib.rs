use libc::{getpwnam_r, passwd};
use pam::constants::PamFlag;
use pam::module::{PamHandle, PamResultCode};
use std::ffi::CString;
use std::ptr;
use trueid_ipc::{send_request, Request, Response};

#[unsafe(no_mangle)]
pub extern "C" fn pam_sm_authenticate(
    pamh: &mut PamHandle,
    _flags: PamFlag,
    _args: Vec<String>,
) -> PamResultCode {
    let user = match pamh.get_user(None) {
        Some(u) => u,
        None => return PamResultCode::PAM_USER_UNKNOWN,
    };

    let uid = match username_to_uid(&user) {
        Some(id) => id,
        None => {
            eprintln!("trueid: failed to resolve uid for user {}", user);
            return PamResultCode::PAM_USER_UNKNOWN;
        }
    };

    match authenticate_via_ipc(uid) {
        Ok(true) => PamResultCode::PAM_SUCCESS,
        Ok(false) => {
            eprintln!("trueid: auth failed for uid {}", uid);
            PamResultCode::PAM_AUTH_ERR
        }
        Err(e) => {
            eprintln!("trueid: IPC error for uid {}: {}", uid, e);
            PamResultCode::PAM_AUTHINFO_UNAVAIL
        }
    }
}

fn username_to_uid(username: &str) -> Option<u32> {
    let c_username = CString::new(username).ok()?;

    let mut pwd: passwd = unsafe { std::mem::zeroed() };

    let raw_buf_size = unsafe { libc::sysconf(libc::_SC_GETPW_R_SIZE_MAX) };
    let buf_size = if raw_buf_size < 0 {
        16 * 1024
    } else {
        raw_buf_size as usize
    };

    let mut buf = vec![0u8; buf_size];
    let mut result: *mut passwd = ptr::null_mut();

    let ret = unsafe {
        getpwnam_r(
            c_username.as_ptr(),
            &mut pwd,
            buf.as_mut_ptr() as *mut libc::c_char,
            buf.len(),
            &mut result,
        )
    };

    if ret == 0 && !result.is_null() {
        let pwd_ref = unsafe { &*result };
        Some(pwd_ref.pw_uid)
    } else {
        None
    }
}

fn authenticate_via_ipc(uid: u32) -> Result<bool, String> {
    let request = Request::Verify { uid };

    match send_request(request) {
        Ok(Response::VerifyResult { accepted }) => Ok(accepted),
        Ok(Response::Error { message }) => Err(message),
        Ok(other) => Err(format!("unexpected IPC response: {:?}", other)),
        Err(e) => Err(e.to_string()),
    }
}