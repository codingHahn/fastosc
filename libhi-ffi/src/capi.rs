use rosc::{OscMessage, OscType};
use std::any::Any;
use std::ffi::{CStr, CString, c_char, c_void};
use std::net::{SocketAddr, SocketAddrV4};
use std::ptr::{self};
use std::str::FromStr;
use std::sync::{Arc, Mutex};

pub use libhi::*;

/// All pointers in rust are !Send. But we need to send an *const c_char over a thread
/// bondary (because callbacks are handled in a seperate thread).
/// This construct wraps `*const c_char` in a newtype and asserts that it is Send
/// See: https://stackoverflow.com/questions/72746732/call-a-c-callback-from-a-send-closure
struct SendCharPtr(*const c_char);
impl SendCharPtr {
    pub fn get_ptr(&self) -> *const c_char {
        self.0
    }
}
unsafe impl Send for SendCharPtr {}

struct SendVoidPtr(*const c_void);
impl SendVoidPtr {
    pub fn get_ptr(&self) -> *const c_void {
        self.0
    }
}
unsafe impl Send for SendVoidPtr {}
impl Copy for SendVoidPtr {}
impl Clone for SendVoidPtr {
    fn clone(&self) -> Self {
        SendVoidPtr(self.0)
    }
}

#[repr(C)]
pub enum ApiResult {
    Success = 0,
    GenericError = -1,
    NullHandleError = -2,
    InvalidArgument = -3,
    MutexFailError = -4,
}

#[unsafe(no_mangle)]
pub extern "C" fn fastosc_server_new(addr: *const c_char) -> *mut OscServer {
    if addr.is_null() {
        return ptr::null_mut();
    }
    let cstring = unsafe { CStr::from_ptr(addr.cast_mut()) };
    let safe_addr = cstring.to_str().unwrap_or("").to_owned();

    if let Ok(working_addr) = SocketAddrV4::from_str(&safe_addr) {
        OscServer::new_from_ip(working_addr)
            .map(|serv| Box::into_raw(Box::new(serv)))
            .unwrap_or(ptr::null_mut())
    } else {
        ptr::null_mut()
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fastosc_server_free(server: *mut OscServer) {
    if !server.is_null() {
        let _ = unsafe { Box::from_raw(server).stop_thread() };
    };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fastosc_register_handler(
    server: *mut OscServer,
    path: *const c_char,
    callback: extern "C" fn(
        *const c_char,
        *const c_char,
        *const SocketAddr,
        *const OscType,
        i32,
        *const c_void,
    ),
    user_data_from_c: *const c_void,
) -> ApiResult {
    let wrapped_path = SendCharPtr(path);
    if let Ok(safe_path) = { unsafe { std::ffi::CStr::from_ptr(path).to_str() } } {
        let callback_translator =
            move |osc_message: &OscMessage,
                  from_addr: &SocketAddr,
                  user_data: Option<Arc<Mutex<Box<dyn Any + Send>>>>| {
                let arg_count = osc_message.args.len() as i32;
                let mut type_str = vec![','];
                for t in &osc_message.args {
                    type_str.push(osc_type_to_char(t.clone()));
                }
                let cs = CString::new(type_str.iter().collect::<String>()).unwrap();
                let type_str_c = SendCharPtr(cs.as_ptr() as *const i8);
                let user_data_c: SendVoidPtr = match user_data.clone() {
                    Some(data) => *data.lock().unwrap().downcast_ref::<SendVoidPtr>().unwrap(),
                    None => SendVoidPtr(ptr::null()),
                };
                (callback)(
                    wrapped_path.get_ptr(),
                    type_str_c.get_ptr(),
                    from_addr as *const SocketAddr,
                    osc_message.args.as_ptr(),
                    arg_count,
                    user_data_c.get_ptr(),
                );
            };
        unsafe {
            match server.as_mut() {
                Some(server) => {
                    let user_data = user_data_from_c.as_ref().map(|user_data| {
                        Arc::new(Mutex::new(
                            Box::new(SendVoidPtr(user_data)) as Box<dyn std::any::Any + Send>
                        ))
                    });
                    if server
                        .register_handler(safe_path, callback_translator, user_data)
                        .is_ok()
                    {
                        return ApiResult::Success;
                    } else {
                        return ApiResult::MutexFailError;
                    }
                }
                None => return ApiResult::NullHandleError,
            }
        }
    }
    ApiResult::InvalidArgument
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fastosc_start_thread(server: *mut OscServer) -> ApiResult {
    unsafe {
        match server.as_mut() {
            Some(server) => {
                if server.start_thread().is_err() {
                    return ApiResult::MutexFailError;
                }
                ApiResult::Success
            }
            None => ApiResult::NullHandleError,
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fastosc_register_error_handler(
    server: *mut OscServer,
    callback: extern "C" fn(*const c_char),
) -> ApiResult {
    let callback_translator = move |err_str: &str| {
        if let Ok(c_str) = CString::new(err_str) {
            (callback)(c_str.as_ptr())
        }
    };
    unsafe {
        match server.as_mut() {
            Some(server) => {
                if server.register_error_handler(callback_translator).is_ok() {
                    ApiResult::Success
                } else {
                    ApiResult::MutexFailError
                }
            }
            None => ApiResult::NullHandleError,
        }
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fastosc_stop_thread(server: *mut OscServer) -> ApiResult {
    unsafe {
        match server.as_mut() {
            Some(server) => {
                if server.stop_thread().is_err() {
                    return ApiResult::MutexFailError;
                }
                ApiResult::Success
            }
            None => ApiResult::NullHandleError,
        }
    }
}
