use libc::{c_char, c_int, getpwnam_r, passwd, uid_t};
use std::ffi::CStr;
use std::ptr;
use trueid_ipc::{Request, Response, send_request};

#[repr(C)]
pub struct pam_handle_t {
    _private: [u8; 0],
}

unsafe extern "C" {
    fn pam_get_user(
        pamh: *mut pam_handle_t,
        user: *mut *const c_char,
        prompt: *const c_char,
    ) -> c_int;
}

const PAM_SUCCESS: c_int = 0;
const PAM_SERVICE_ERR: c_int = 3;
const PAM_AUTH_ERR: c_int = 7;
const PAM_USER_UNKNOWN: c_int = 10;

fn lookup_uid(username: &CStr) -> Result<uid_t, c_int> {
    let mut pwd = passwd {
        pw_name: ptr::null_mut(),
        pw_passwd: ptr::null_mut(),
        pw_uid: 0,
        pw_gid: 0,
        pw_gecos: ptr::null_mut(),
        pw_dir: ptr::null_mut(),
        pw_shell: ptr::null_mut(),
    };
    let mut result: *mut passwd = ptr::null_mut();
    let mut buffer = vec![0_u8; 16 * 1024];

    let status = unsafe {
        getpwnam_r(
            username.as_ptr(),
            &mut pwd,
            buffer.as_mut_ptr().cast(),
            buffer.len(),
            &mut result,
        )
    };

    if status != 0 {
        return Err(PAM_SERVICE_ERR);
    }

    if result.is_null() {
        return Err(PAM_USER_UNKNOWN);
    }

    Ok(pwd.pw_uid)
}

fn authenticate_username(username: &CStr) -> c_int {
    let uid = match lookup_uid(username) {
        Ok(uid) => uid,
        Err(code) => return code,
    };

    match send_request(Request::Verify { uid }) {
        Ok(Response::VerifyResult { accepted: true }) => PAM_SUCCESS,
        Ok(Response::VerifyResult { accepted: false }) => PAM_AUTH_ERR,
        Ok(Response::Error { .. }) => PAM_AUTH_ERR,
        Ok(_) => PAM_SERVICE_ERR,
        Err(_) => PAM_SERVICE_ERR,
    }
}

/// # Safety
///
/// PAM calls this entrypoint with a valid `pam_handle_t` for the active
/// authentication transaction and the standard module ABI arguments.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pam_sm_authenticate(
    pamh: *mut pam_handle_t,
    _flags: c_int,
    _argc: c_int,
    _argv: *const *const c_char,
) -> c_int {
    let mut user_ptr: *const c_char = ptr::null();
    let status = unsafe { pam_get_user(pamh, &mut user_ptr, ptr::null()) };

    if status != PAM_SUCCESS {
        return status;
    }

    if user_ptr.is_null() {
        return PAM_USER_UNKNOWN;
    }

    let username = unsafe { CStr::from_ptr(user_ptr) };

    if username.to_bytes().is_empty() {
        return PAM_USER_UNKNOWN;
    }

    authenticate_username(username)
}

#[unsafe(no_mangle)]
pub extern "C" fn pam_sm_setcred(
    _pamh: *mut pam_handle_t,
    _flags: c_int,
    _argc: c_int,
    _argv: *const *const c_char,
) -> c_int {
    PAM_SUCCESS
}
