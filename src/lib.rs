use rosc::address::{Matcher, OscAddress};
use rosc::{OscError, OscMessage, OscPacket, OscType};
use std::collections::HashMap;
use std::ffi::{CString, c_char};
use std::io::Error;
use std::net::{SocketAddr, SocketAddrV4, UdpSocket};
use std::ptr::{self};
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

#[derive(Debug)]
pub enum FastOscError {
    ReadError(std::io::Error),
    SendError(std::io::Error),
    OscError(OscError),
    MutexFail,
    RegisterHandlerError,
    ThreadPanic,
}

/// Maps an `OScAddress` to a closure which has to be sent,
/// because it is called in another thread
type AddressToCallbackMap = HashMap<OscAddress, Box<dyn Fn(&OscMessage, &SocketAddr) + Send>>;

/// Internal, non threadsafe OscServer. It gets wrapped in `Arc` and `Mutex` in
/// `OscServer`
struct OscServerInternal {
    sock: UdpSocket,
    message_handlers: AddressToCallbackMap,
    error_handler: Option<Box<dyn Fn(&str) + Send>>,
    thread_handle: Option<JoinHandle<()>>,
    stop: bool,
}

impl OscServerInternal {
    fn new(sock: UdpSocket) -> Self {
        OscServerInternal {
            sock,
            message_handlers: HashMap::new(),
            error_handler: None,
            thread_handle: None,
            stop: false,
        }
    }

    fn register_handler(
        &mut self,
        path: &OscAddress,
        callback: impl Fn(&OscMessage, &SocketAddr) + 'static + Send,
    ) {
        self.message_handlers
            .insert(path.to_owned(), Box::new(callback));
    }

    fn register_error_handler(&mut self, callback: impl Fn(&str) + 'static + Send) {
        self.error_handler = Some(Box::new(callback));
    }

    fn recv(&self) -> Result<(), OscError> {
        let mut buf = [0u8; rosc::decoder::MTU];
        match self.sock.recv_from(&mut buf) {
            Ok((size, addr)) => {
                println!("Received packet with size {} from: {}", size, addr);
                let (_, packet) = rosc::decoder::decode_udp(&buf[..size])?;
                self.handle_packet(packet, addr)?;
                Ok(())
            }
            Err(e) => {
                println!("Error receiving from socket: {}", e);
                Err(e).map_err(|_| OscError::BadPacket("Error recieving from socket"))
            }
        }
    }

    fn handle_packet(&self, packet: OscPacket, from_addr: SocketAddr) -> Result<(), OscError> {
        match packet {
            OscPacket::Message(msg) => {
                if let Ok(address) = OscAddress::new(msg.addr.clone()) {
                    if let Some(callback) = self.message_handlers.get(&address) {
                        (callback)(&msg, &from_addr);
                        return Ok(());
                    }
                } else if let Ok(matcher) = Matcher::new(&msg.addr) {
                    for (addr, handler) in self.message_handlers.iter() {
                        if matcher.match_address(addr) {
                            (handler)(&msg, &from_addr);
                        }
                    }
                    return Ok(());
                }
                println!("No handler for path: {0}", msg.addr);
                Err(OscError::Unimplemented)
            }

            OscPacket::Bundle(bundle) => {
                for item in bundle.content {
                    self.handle_packet(item, from_addr)?;
                }
                Ok(())
            }
        }
    }

    pub fn send_packet(&self, packet: OscPacket, to_addr: SocketAddr) -> Result<(), FastOscError> {
        let buf = rosc::encoder::encode(&packet).map_err(|e| FastOscError::OscError(e))?;
        self.sock
            .send_to(&buf, to_addr)
            .map_err(|e| FastOscError::SendError(e))?;
        Ok(())
    }
}

#[repr(C)]
#[derive(Clone)]
pub struct OscServer {
    internal: Arc<Mutex<OscServerInternal>>,
}

impl OscServer {
    pub fn new_from_ip(addr: SocketAddrV4) -> Result<Self, Error> {
        let sock = UdpSocket::bind(addr)?;
        let intern = Arc::new(Mutex::new(OscServerInternal::new(sock)));
        Ok(OscServer { internal: intern })
    }

    pub fn register_handler(
        &self,
        path: &str,
        callback: impl Fn(&OscMessage, &SocketAddr) + 'static + Send,
    ) -> Result<(), FastOscError> {
        if let Ok(addr) = OscAddress::new(path.to_owned()) {
            self.internal
                .lock()
                .map_err(|_| FastOscError::RegisterHandlerError)?
                .register_handler(&addr, callback);
            return Ok(());
        }
        Err(FastOscError::RegisterHandlerError)
    }

    pub fn register_error_handler(
        &self,
        callback: impl Fn(&str) + 'static + Send,
    ) -> Result<(), FastOscError> {
        self.internal
            .lock()
            .map_err(|_| FastOscError::MutexFail)?
            .register_error_handler(callback);
        Ok(())
    }

    pub fn recv(&self) -> Result<(), FastOscError> {
        self.internal
            .lock()
            .map_err(|_| FastOscError::MutexFail)?
            .recv()
            .map_err(FastOscError::OscError)
    }

    pub fn handle_packet(
        &self,
        packet: OscPacket,
        from_addr: SocketAddr,
    ) -> Result<(), FastOscError> {
        self.internal
            .lock()
            .map_err(|_| FastOscError::RegisterHandlerError)?
            .handle_packet(packet, from_addr)
            .map_err(FastOscError::OscError)
    }

    pub fn send_packet(&self, packet: OscPacket, to_addr: SocketAddr) -> Result<(), FastOscError> {
        let lock = self.internal.lock().map_err(|_| FastOscError::MutexFail)?;

        lock.send_packet(packet, to_addr)
    }

    pub fn start_thread(&mut self) -> Result<(), FastOscError> {
        let serv = self.clone();
        let server_handle = std::thread::spawn(move || {
            loop {
                if let Err(err) = serv.recv() {
                    if let Ok(lock) = serv.internal.lock() {
                        if lock.stop {
                            break;
                        }
                        match &lock.error_handler {
                            Some(handler) => {
                                let error_str = format!("FastOscError: {err:#?}");
                                (handler)(&error_str);
                            }
                            None => todo!(),
                        }
                    }
                }
            }
        });
        self.internal
            .lock()
            .map_err(|_| FastOscError::MutexFail)?
            .thread_handle = Some(server_handle);
        Ok(())
    }

    pub fn stop_thread(&mut self) -> Result<(), FastOscError> {
        let mut lock = self
            .internal
            .lock()
            .map_err(|_| FastOscError::RegisterHandlerError)?;

        lock.stop = true;
        if let Some(th) = lock.thread_handle.take() {
            th.join().map_err(|_| FastOscError::ThreadPanic)?;
        };

        // Reset the stop condition variable to allow starting the thread again
        lock.stop = false;

        Ok(())
    }
}

fn osc_type_to_char(osc_type: OscType) -> char {
    match osc_type {
        OscType::Int(_) => 'i',
        OscType::Float(_) => 'f',
        OscType::String(_) => 's',
        OscType::Blob(_) => 'b',
        OscType::Time(_) => 't',
        OscType::Long(_) => 'h',
        OscType::Double(_) => 'd',
        OscType::Char(_) => 'c',
        OscType::Color(_) => 'r',
        OscType::Midi(_) => 'm',
        OscType::Bool(val) => match val {
            true => 'T',
            false => 'F',
        },
        OscType::Array(_) => todo!(),
        OscType::Nil => 'N',
        OscType::Inf => 'I',
    }
}

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
    let cstring = unsafe { CString::from_raw(addr.cast_mut()) };
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
    callback: extern "C" fn(*const c_char, *const c_char, *const SocketAddr, *const OscType, i32),
) -> ApiResult {
    let wrapped_path = SendCharPtr(path);
    if let Ok(safe_path) = { unsafe { std::ffi::CStr::from_ptr(path).to_str() } } {
        let callback_translator = move |osc_message: &OscMessage, from_addr: &SocketAddr| {
            let arg_count = osc_message.args.len() as i32;
            let mut type_str = vec![','];
            for t in &osc_message.args {
                type_str.push(osc_type_to_char(t.clone()));
            }
            let type_str_c = SendCharPtr(type_str.iter().collect::<String>().as_ptr() as *const i8);
            (callback)(
                wrapped_path.get_ptr(),
                type_str_c.get_ptr(),
                from_addr as *const SocketAddr,
                osc_message.args.as_ptr(),
                arg_count,
            );
        };
        unsafe {
            match server.as_mut() {
                Some(server) => {
                    if server
                        .register_handler(safe_path, callback_translator)
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
