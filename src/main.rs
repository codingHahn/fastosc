use rosc::OscMessage;
use std::env;
use std::net::{SocketAddr, SocketAddrV4};
use std::str::FromStr;
use std::time::Duration;

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

    let mut server = OscServer::new_from_ip(addr).expect("Error creating OscServer");
    server.start_thread().expect("Error creating thread");
    println!("Thread spawned");
    server
        .register_handler(
            "/test",
            Box::new(|msg: &OscMessage, _from_addr: &SocketAddr| {
                println!("Hi, {0}, {1:#?}", msg.addr, msg.args)
            }),
        )
        .unwrap();
    server
        .register_handler(
            "/test2",
            Box::new(|msg: &OscMessage, _from_addr: &SocketAddr| {
                println!("Hi, {0}, {1:#?}", msg.addr, msg.args)
            }),
        )
        .unwrap();
    server
        .register_handler(
            "/test5",
            Box::new(|msg: &OscMessage, _from_addr: &SocketAddr| {
                println!("Hi, {0}, {1:#?}", msg.addr, msg.args)
            }),
        )
        .unwrap();
    loop {
        std::thread::sleep(Duration::from_secs(1))
    }
}
