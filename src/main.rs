use rosc::OscMessage;
use std::env;
use std::net::SocketAddrV4;
use std::str::FromStr;

use libhi::OscServer;

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

    let server = OscServer::new_from_ip(addr).unwrap();
    let thread_server = server.clone();
    let server_handle = std::thread::spawn(move || {
        loop {
            thread_server.recv().unwrap()
        }
    });
    println!("Thread spawned");
    server.register_handler(
        "/test",
        Box::new(|msg: &OscMessage| println!("Hi, {0}, {1:#?}", msg.addr, msg.args)),
    );
    server.register_handler(
        "/test2",
        Box::new(|msg: &OscMessage| println!("Hi, {0}, {1:#?}", msg.addr, msg.args)),
    );
    server.register_handler(
        "/test5",
        Box::new(|msg: &OscMessage| println!("Hi, {0}, {1:#?}", msg.addr, msg.args)),
    );

    let _ = server_handle.join();
}
