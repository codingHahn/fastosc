//! This crate is inspired by the C API from liblo.
use libhi::rosc::address::OscAddress;
use rosc::OscType;
use std::any::Any;
use std::ffi::{self, CStr, CString, c_char, c_float, c_int, c_uint, c_ushort, c_void};
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

struct SendVoidPtr(*mut c_void);
impl SendVoidPtr {
    pub fn get_mut_ptr(&self) -> *mut c_void {
        self.0
    }
}
unsafe impl Send for SendVoidPtr {}
impl Copy for SendVoidPtr {}
impl Clone for SendVoidPtr {
    fn clone(&self) -> Self {
        *self
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

/// Constructs and returns an [`OscServer`] that is configured to listen on the given IPv4 address
/// and port combination in the following style: IPv4:Port, for example `127.0.0.1:50014`.
///
/// Returns a nullptr if the address is invalid or already in use.
///
/// To start the server thread (and actually recieve messages, call [`hi_start_thread`].
/// To free the server, call [`hi_server_free`].
#[unsafe(no_mangle)]
pub extern "C" fn hi_server_new(addr: *const c_char) -> *mut OscServer {
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

/// Frees a server created by [`hi_server_new`].
///
/// # Safety
///
/// Calling this function with a pointer that was not allocated by [`hi_server_new`] is UB and
/// may lead to a segfault.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hi_server_free(server: *mut OscServer) {
    if !server.is_null() {
        let _ = unsafe { Box::from_raw(server).stop_thread() };
    };
}

/// Add an int response to the [`OscAnswer`].
///
/// # Safety
/// `answer` must be a valid [`OscAnswer`] passed to the callback.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hi_answer_add_int(answer: *mut OscAnswer, arg: c_int) -> ApiResult {
    unsafe {
        match answer.as_mut() {
            Some(ans) => {
                ans.add_argument(OscType::Int(arg));
                ApiResult::Success
            }
            None => ApiResult::NullHandleError,
        }
    }
}

/// Add a float response to the [`OscAnswer`].
///
/// # Safety
/// `answer` must be a valid [`OscAnswer`] passed to the callback.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hi_answer_add_float(answer: *mut OscAnswer, arg: c_float) -> ApiResult {
    unsafe {
        match answer.as_mut() {
            Some(ans) => {
                ans.add_argument(OscType::Float(arg));
                ApiResult::Success
            }
            None => ApiResult::NullHandleError,
        }
    }
}

/// Add a string response to the [`OscAnswer`].
///
/// # Safety
/// `answer` must be a valid [`OscAnswer`] passed to the callback.
/// If the string is not valid, it is replaced by an empty string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hi_answer_add_string(
    answer: *mut OscAnswer,
    arg: *const c_char,
) -> ApiResult {
    unsafe {
        match answer.as_mut() {
            Some(ans) => {
                let cstring = CStr::from_ptr(arg.cast_mut());
                let safe_arg = cstring.to_str().unwrap_or("").to_owned();
                ans.add_argument(OscType::String(safe_arg));
                ApiResult::Success
            }
            None => ApiResult::NullHandleError,
        }
    }
}

/// Set the port the [`OscAnswer`] is sent back to.
///
/// # Safety
/// `answer` must be a valid [`OscAnswer`] passed to the callback.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hi_answer_set_port(answer: *mut OscAnswer, port: c_ushort) -> ApiResult {
    unsafe {
        match answer.as_mut() {
            Some(ans) => {
                ans.set_port(port);
                ApiResult::Success
            }
            None => ApiResult::NullHandleError,
        }
    }
}

/// Marks an [`OscAnswer`] to be sent after the callback returns.
///
/// # Safety
/// `answer` must be a valid [`OscAnswer`] passed to the callback.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hi_answer_mark_send(
    answer: *mut OscAnswer,
    will_be_sent: bool,
) -> ApiResult {
    unsafe {
        match answer.as_mut() {
            Some(ans) => {
                ans.mark_send(will_be_sent);
                ApiResult::Success
            }
            None => ApiResult::NullHandleError,
        }
    }
}

/// Register a handler that responds to the address specified in the argument `path`.
///
/// `user_data_from_c` can either be `NULL` or an arbitrary pointer to some data that
/// gets passed to the callback as last argument.
///
/// # Callback
///
/// The callback is the most advanced argument in this list. It gets the following arguments passed
/// in that order:
///
/// - osc_address as `const char*`: The address the handler got called for
/// - osc_types as `const char*`: A string of the recieved arguments in the OSC message, starting
///   with `,`
/// - osc_arguments as `const void**`: A list of arguments in the message with length `len`
/// - len as `int_32`: Length of the osc_arguments list
/// - osc_answer as `OscAnswer*`: An OSC answer prepopulated with the osc address and return ip
///   address. Can be modified by the `hi_answer` functions
/// - user_data as `void *`: `user_data_from_c` will be passed to this
///
/// # Safety
///
/// The function pointer `callback` is called from another thread (started by
/// [`hi_start_thread`], so make sure that everything you do in the callback is
/// thread-safe.
/// Also make sure that the data in `user_data_from_c` is accessed in a thread-safe way
/// for the same reason.
/// All arguments are only valid during the execution of the callback, with the exeption of
/// `user_data_from_c`, where the lifetime is controlled by the caller of this function.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hi_register_handler(
    server: *mut OscServer,
    path: *const c_char,
    types: *const c_char,
    callback: extern "C" fn(
        *const c_char,
        *const c_char,
        *const *const c_void,
        i32,
        *mut OscAnswer,
        *mut c_void,
    ),
    user_data_from_c: *mut c_void,
) -> ApiResult {
    let wrapped_path = SendCharPtr(path);
    if let Ok(safe_path) = { unsafe { std::ffi::CStr::from_ptr(path).to_str() } }
        && let Ok(safe_types) = {
            unsafe {
                if types.is_null() {
                    Ok("")
                } else {
                    core::ffi::CStr::from_ptr(types).to_str()
                }
            }
        }
    {
        let callback_translator =
            move |_osc_addr: &OscAddress,
                  osc_args: &Vec<OscType>,
                  answer: &mut OscAnswer,
                  user_data: Option<Arc<Mutex<Box<dyn Any + Send>>>>| {
                let mut type_str = vec![];
                for t in osc_args {
                    type_str.push(osc_type_to_char(t));
                }
                let cs = CString::new(type_str.iter().collect::<String>()).unwrap();
                let type_str_c = SendCharPtr(cs.as_ptr());
                let user_data_c: SendVoidPtr = match user_data.clone() {
                    Some(data) => *data.lock().unwrap().downcast_ref::<SendVoidPtr>().unwrap(),
                    None => SendVoidPtr(ptr::null_mut()),
                };
                let c_args: Vec<*const c_void> = osc_args.iter().map(osctype_to_void_ptr).collect();
                (callback)(
                    wrapped_path.get_ptr(),
                    type_str_c.get_ptr(),
                    c_args.as_ptr(),
                    c_args.len() as i32,
                    answer as *mut OscAnswer,
                    user_data_c.get_mut_ptr(),
                );
                // Prevent memory leak by owning all strings from c_args again.
                // They were allocated by CString::into_raw which leaks without ::from_raw
                for (c, arg) in std::iter::zip(type_str, c_args) {
                    if c == 's' {
                        unsafe {
                            let _ = CString::from_raw(arg as *mut c_char);
                        }
                    }
                }
            };
        unsafe {
            match server.as_mut() {
                Some(server) => {
                    let user_data = user_data_from_c.as_mut().map(|user_data| {
                        Arc::new(Mutex::new(
                            Box::new(SendVoidPtr(user_data)) as Box<dyn std::any::Any + Send>
                        ))
                    });
                    if server
                        .register_handler(safe_path, safe_types, callback_translator, user_data)
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

/// Spawn a new thread and listen to the address specified in [`hi_server_new`].
/// The thread can be stopped again using [`hi_stop_thread`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hi_start_thread(server: *mut OscServer) -> ApiResult {
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
pub unsafe extern "C" fn hi_register_error_handler(
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

/// Stop the server thread started by [`hi_start_thread`] safely. The message that is
/// currently processing will be finished, after that the thread exits. A stopped thread can be
/// started again.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hi_stop_thread(server: *mut OscServer) -> ApiResult {
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

#[unsafe(no_mangle)]
pub unsafe extern "C" fn hi_get_ip_of_socket_addr(sock_addr: *const SocketAddr) -> *const char {
    unsafe {
        match sock_addr.as_ref() {
            Some(sock_addr) => {
                CString::into_raw(CString::new(sock_addr.ip().to_string()).unwrap()) as *const char
            }
            None => ptr::null(),
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn hi_free_ip_of_socket_addr(addr_part: *mut char) {
    unsafe {
        if addr_part.is_null() {
            return;
        }
        // Reconstruct a CString from the pointer and drop it
        let _ = CString::from_raw(addr_part as *mut i8);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn hi_get_port_of_socket_addr(sock_addr: *const SocketAddr) -> u16 {
    unsafe {
        match sock_addr.as_ref() {
            Some(sock_addr) => sock_addr.port(),
            None => 0,
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn hi_set_port_of_socket_addr(
    sock_addr: *mut SocketAddr,
    port: u16,
) -> ApiResult {
    unsafe {
        match sock_addr.as_mut() {
            Some(sock_addr) => {
                sock_addr.set_port(port);
                ApiResult::Success
            }
            None => ApiResult::NullHandleError,
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn hi_version(
    verstr: *mut c_char,
    verstr_size: c_uint,
    major: *mut c_int,
    minor: *mut c_int,
    patch: *mut c_int,
) {
    let (version_str, major_ver, minor_ver, patch_ver) = library_version();

    unsafe {
        if !verstr.is_null() && version_str.len() < verstr_size as usize {
            let str_src = CString::new(version_str).unwrap_or_default();
            let len = std::cmp::min(str_src.as_bytes_with_nul().len(), verstr_size as usize);
            std::ptr::copy_nonoverlapping(str_src.as_ptr(), verstr, len);
        }
        if let Some(c_major) = major.as_mut() {
            *c_major = major_ver
        }
        if let Some(c_minor) = minor.as_mut() {
            *c_minor = minor_ver
        }
        if let Some(c_patch) = patch.as_mut() {
            *c_patch = patch_ver
        }
    }
}

#[unsafe(no_mangle)]
fn osctype_to_void_ptr(t: &OscType) -> *const c_void {
    match t {
        OscType::Int(i) => i as *const ffi::c_int as *const c_void,
        OscType::Float(i) => i as *const ffi::c_float as *const c_void,
        OscType::String(i) => CString::new(i.as_str()).unwrap().into_raw() as *const c_void,
        OscType::Blob(_) => todo!(),
        OscType::Long(i) => i as *const ffi::c_long as *const c_void,
        OscType::Double(i) => i as *const ffi::c_double as *const c_void,
        OscType::Char(i) => {
            <char as TryInto<u8>>::try_into(*i).unwrap() as *const u8 as *const c_void
        }
        OscType::Bool(i) => i as *const bool as *const c_void,
        _ => todo!(),
    }
}
