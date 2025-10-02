use rosc::OscType;
use rosc::address::OscAddress;
use std::env;
use std::net::SocketAddrV4;
use std::str::FromStr;
use std::time::Duration;

use libhi::{OscAnswer, OscServer};

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
            "",
            Box::new(
                |path: &OscAddress,
                 args: &Vec<OscType>,
                 _answer: &mut OscAnswer,
                 _user_data: Option<_>| { println!("Hi, {0}, {1:#?}", path, args) },
            ),
            None,
        )
        .unwrap();
    server
        .register_handler(
            "/test2",
            "",
            Box::new(
                |path: &OscAddress,
                 args: &Vec<OscType>,
                 _answer: &mut OscAnswer,
                 _user_data: Option<_>| { println!("Hi, {0}, {1:#?}", path, args) },
            ),
            None,
        )
        .unwrap();
    server
        .register_handler(
            "/with_response",
            "i",
            Box::new(
                |_path: &OscAddress,
                 args: &Vec<OscType>,
                 answer: &mut OscAnswer,
                 _user_data: Option<_>| {
                    answer.replace_arguments(args.to_vec());
                    answer.set_port(50010);
                    answer.mark_send(true);
                    println!("Hi");
                },
            ),
            None,
        )
        .unwrap();
    loop {
        std::thread::sleep(Duration::from_secs(1))
    }
}
