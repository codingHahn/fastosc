use rosc::{OscError, OscMessage, OscPacket, OscType};
use std::collections::HashMap;
use std::ffi::{CString, OsString};
use std::io::Error;
use std::net::{SocketAddrV4, UdpSocket};
use std::ptr::null;
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::{env, ffi};

enum FastOscError {
    ReadError(std::io::Error),
    SendError(std::io::Error),
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let usage = format!("Usage {} IP:PORT", &args[0]);
    if args.len() < 2 {
        println!("{}", usage);
        ::std::process::exit(1)
    }
    let addr = match SocketAddrV4::from_str(&args[1]) {
        Ok(addr) => addr,
        Err(_) => panic!("{}", usage),
    };

    let server = OscServer::new_from_ip(addr);
    let mult_server = Arc::new(server);
    let thread_server = mult_server.clone();
    let server_handle = std::thread::spawn(move || {
        loop {
            thread_server.recv().unwrap()
        }
    });
    println!("Thread spawned");
    mult_server.register_handler(
        "/test",
        Box::new(|msg: &OscMessage| println!("Hi, {0}, {1:#?}", msg.addr, msg.args)),
    );
    mult_server.register_handler(
        "/test2",
        Box::new(|msg: &OscMessage| println!("Hi, {0}, {1:#?}", msg.addr, msg.args)),
    );
    mult_server.register_handler(
        "/test5",
        Box::new(|msg: &OscMessage| println!("Hi, {0}, {1:#?}", msg.addr, msg.args)),
    );

    let _ = server_handle.join();
}

#[repr(C)]
pub struct OscServer {
    sock: UdpSocket,
    message_handlers: Mutex<HashMap<String, Box<dyn Fn(&OscMessage) + Send>>>,
}

impl OscServer {
    fn new_from_ip(addr: SocketAddrV4) -> Self {
        Self {
            sock: UdpSocket::bind(addr).unwrap(),
            message_handlers: HashMap::new().into(),
        }
    }

    fn register_handler(&self, path: &str, callback: impl Fn(&OscMessage) + 'static + Send) {
        self.message_handlers
            .lock()
            .unwrap()
            .insert(path.to_owned(), Box::new(callback));
    }

    fn recv(&self) -> Result<(), OscError> {
        let mut buf = [0u8; rosc::decoder::MTU];
        match self.sock.recv_from(&mut buf) {
            Ok((size, addr)) => {
                println!("Received packet with size {} from: {}", size, addr);
                let (_, packet) = rosc::decoder::decode_udp(&buf[..size])?;
                self.handle_packet(packet)?;
                Ok(())
            }
            Err(e) => {
                println!("Error receiving from socket: {}", e);
                Err(e).map_err(|_| OscError::BadPacket("Error recieving from socket"))
            }
        }
    }

    fn handle_packet(&self, packet: OscPacket) -> Result<(), OscError> {
        match packet {
            OscPacket::Message(msg) => {
                let mutex = self.message_handlers.lock().map_err(|_| OscError::Unimplemented)?;
                if let Some(callback) = mutex.get(&msg.addr) {
                    (callback)(&msg);
                    Ok(())
                } else {
                    println!("No handler for path: {0}", msg.addr);
                    Err(OscError::Unimplemented)
                }
            }
            OscPacket::Bundle(bundle) => {
                for item in bundle.content {
                    self.handle_packet(item)?;
                }
                Ok(())
            }
        }
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

pub extern "C" fn fastosc_server_new(
    server: *mut OscServer,
    addr: *const std::ffi::c_char,
) -> isize {
    if let Ok(safe_addr) = unsafe { std::ffi::CStr::from_ptr(addr).to_str() } {
        if let Ok(working_addr) = SocketAddrV4::from_str(safe_addr) {
            unsafe {
                *server = OscServer::new_from_ip(working_addr);
            }
            return 0;
        } else {
            return -1;
        };
    } else {
        return -1;
    }
}

pub extern "C" fn fastosc_register_handler(
    server: *mut OscServer,
    path: *const std::ffi::c_char,
    callback: extern "C" fn(*const std::ffi::c_char, *const std::ffi::c_char, *const OscType, i32),
) {
    if let Ok(safe_path) = unsafe { std::ffi::CStr::from_ptr(path).to_str() } {
        let callback_translator = move |osc_message: &OscMessage| {
            let c_addr = CString::new(osc_message.addr.clone()).unwrap().as_ptr();
            let arg_count = osc_message.args.iter().count() as i32;
            let mut type_str = vec![','];
            for t in &osc_message.args {
                type_str.push(osc_type_to_char(t.clone()));
            }
            let type_str_c = type_str.iter().collect::<String>().as_ptr();
            (callback)(
                c_addr,
                type_str_c as *const i8,
                osc_message.args.as_ptr(),
                arg_count,
            );
        };
        unsafe {
            match server.as_ref() {
                Some(serv) => serv.register_handler(safe_path, callback_translator),
                None => (),
            }
        }
    }
}

