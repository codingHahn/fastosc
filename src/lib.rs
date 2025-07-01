use rosc::address::{Matcher, OscAddress};
use rosc::{OscError, OscMessage, OscPacket, OscType};
use std::any::Any;
use std::collections::HashMap;
use std::io::Error;
use std::net::{SocketAddr, SocketAddrV4, UdpSocket};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

pub use rosc;

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
type AddressToCallbackMap = HashMap<OscAddress, CallbackWithUserdata>;

struct CallbackWithUserdata {
    callback: Box<
        dyn Fn(&OscMessage, &SocketAddr, Option<Arc<Mutex<Box<dyn Any + Send>>>>) + Send + 'static,
    >,
    user_data: Option<Arc<Mutex<Box<dyn Any + Send>>>>,
}
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
        callback: impl Fn(&OscMessage, &SocketAddr, Option<Arc<Mutex<Box<dyn Any + Send>>>>)
        + 'static
        + Send,
        user_data: Option<Arc<Mutex<Box<dyn Any + Send>>>>,
    ) {
        let callback_data = CallbackWithUserdata {
            callback: Box::new(callback),
            user_data,
        };
        self.message_handlers.insert(path.to_owned(), callback_data);
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
                    if let Some(callback_with_userdata) = self.message_handlers.get(&address) {
                        (callback_with_userdata.callback)(
                            &msg,
                            &from_addr,
                            callback_with_userdata.user_data.clone(),
                        );
                        return Ok(());
                    }
                } else if let Ok(matcher) = Matcher::new(&msg.addr) {
                    for (addr, handler) in self.message_handlers.iter() {
                        if matcher.match_address(addr) {
                            (handler.callback)(&msg, &from_addr, handler.user_data.clone());
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
        let buf = rosc::encoder::encode(&packet).map_err(FastOscError::OscError)?;
        self.sock
            .send_to(&buf, to_addr)
            .map_err(FastOscError::SendError)?;
        Ok(())
    }
}

/// OSC server instance which holds data about, such as the thread handle, address handlers and the
/// UDP socket. Instances of this object can be cloned and copied, because the data is protected by
/// an [`Arc`] [`Mutex`].
#[derive(Clone)]
pub struct OscServer {
    internal: Arc<Mutex<OscServerInternal>>,
}

impl OscServer {
    /// Build and return a new instance of [`OscServer`] from an [`SocketAddrV4`]. This is fails
    /// when it can't bind to the IP. This happens if the IP/port combination is already in use or
    /// it can't bind to the IP.
    pub fn new_from_ip(addr: SocketAddrV4) -> Result<Self, Error> {
        let sock = UdpSocket::bind(addr)?;
        let intern = Arc::new(Mutex::new(OscServerInternal::new(sock)));
        Ok(OscServer { internal: intern })
    }

    /// Registers a closure `callback` for a given OSC address `path`. Currently, only addresses
    /// without address patterns are supported.
    /// Optionally, arbitrary data `user_data` can be provided, which will be avalable to the
    /// callback closure when it is called.
    pub fn register_handler(
        &self,
        path: &str,
        callback: impl Fn(&OscMessage, &SocketAddr, Option<Arc<Mutex<Box<dyn Any + Send>>>>)
        + 'static
        + Send,
        user_data: Option<Arc<Mutex<Box<dyn Any + Send>>>>,
    ) -> Result<(), FastOscError> {
        if let Ok(addr) = OscAddress::new(path.to_owned()) {
            self.internal
                .lock()
                .map_err(|_| FastOscError::RegisterHandlerError)?
                .register_handler(&addr, callback, user_data);
            return Ok(());
        }
        Err(FastOscError::RegisterHandlerError)
    }

    /// Registers an error closure `callback` which will be called with the error message. This is
    /// optional.
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

    /// Starts an OSC server with the configured options in a seperate thread. It is still possible
    /// to modify the handlers and other configuration while the thread is running.
    /// Please note: The UDP socket is opened upon creation of the [`OscServer`] object. Packets
    /// recieved after creating the object and calling this method will be processed immedeatly
    /// after calling this method.
    ///
    /// It is possible to stop the thread using [`OscServer::stop_thread`].
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

    /// Stops the thread started by [`OscServer::start_thread`]. The thread will finish processing
    /// a packet if it is in the middle of doing so.
    /// A thread can be started again by calling `start_thread`.
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

pub fn osc_type_to_char(osc_type: OscType) -> char {
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
