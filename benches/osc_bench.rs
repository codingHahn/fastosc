use criterion::BenchmarkId;
use criterion::Criterion;
use criterion::Throughput;
use criterion::{criterion_group, criterion_main};
use libhi::OscAnswer;
use libhi::OscServer;
use rosc::address::OscAddress;
use rosc::{OscMessage, OscPacket, OscType};
use std::fs::File;
use std::io::{BufReader, prelude::*};
use std::net::{SocketAddr, SocketAddrV4};
use std::str::FromStr;

/// Benchmark how long it takes to dispatch an OSC message to the corresponding handler.
/// This benchmarks purely the handler dispatch and does not include the UDP socket overhead.
/// In this benchmark, [1, 10, 100, 1000, 10_000] handlers get registered and then a message is
/// dispatched, measuring the time it takes to call the handler.
fn bench_handler_dispatch(c: &mut Criterion) {
    let addr = SocketAddrV4::from_str("127.0.0.1:50000").unwrap();

    let mut group = c.benchmark_group("bench_handler_dispatch");
    for size in [1, 100, 1000, 10_000].iter() {
        let file = File::open("./benches/example_addrs.txt").unwrap();
        let reader = BufReader::new(file);
        let server = OscServer::new_from_ip(addr).expect("Error creating OscServer");

        for (count, line) in reader.lines().enumerate() {
            if count > *size {
                break;
            }
            let line = line.unwrap();
            server
                .register_handler(
                    &line,
                    "",
                    Box::new(
                        |_path: &OscAddress,
                         args: &Vec<OscType>,
                         _answer: &mut OscAnswer,
                         _user_data: Option<_>| {
                            std::hint::black_box(args);
                        },
                    ),
                    None,
                )
                .unwrap();
            server
                .register_handler(
                    &line,
                    "",
                    Box::new(
                        |_path: &OscAddress,
                         args: &Vec<OscType>,
                         _answer: &mut OscAnswer,
                         _user_data: Option<_>| {
                            std::hint::black_box(args);
                        },
                    ),
                    None,
                )
                .unwrap();
        }
        let packet = OscPacket::Message(OscMessage {
            addr: "/dbaudio1/fixed/hardwarevariant".to_string(),
            args: vec![OscType::Int(1)],
        });
        group.throughput(Throughput::Elements(*size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &_size| {
            b.iter(|| server.handle_packet(packet.clone(), SocketAddr::V4(addr)));
        });
    }
    group.finish();
}

/// Benchmark the performance of OSC address patterns. The current (naive) implementation
/// requires every handler to be matched to the address pattern. If the handler matches,
/// it gets executed.
/// This benchmarks models the `/dbaudio1/matrixnode/enable` endpoints of the d&b DS100.
/// There are 8192 handlers which begin with /dbaudio1/matrixnode/enable in this benchmark.
/// The OSC address with wildcard we test here matches 64 handlers.
fn bench_handler_dispatch_wildcard(c: &mut Criterion) {
    let addr = SocketAddrV4::from_str("127.0.0.1:50000").unwrap();

    let mut group = c.benchmark_group("bench_handler_dispatch_wildcard");
    for size in [1, 100, 1000, 10_000].iter() {
        let file = File::open("./benches/wildcard_stress.txt").unwrap();
        let reader = BufReader::new(file);
        let server = OscServer::new_from_ip(addr).expect("Error creating OscServer");

        for (count, line) in reader.lines().enumerate() {
            if count > *size {
                break;
            }
            let line = line.unwrap();
            server
                .register_handler(
                    &line,
                    "",
                    Box::new(
                        |_path: &OscAddress,
                         args: &Vec<OscType>,
                         _answer: &mut OscAnswer,
                         _user_data: Option<_>| {
                            std::hint::black_box(args);
                        },
                    ),
                    None,
                )
                .unwrap();
        }
        let packet = OscPacket::Message(OscMessage {
            addr: "/dbaudio1/matrixnode/enable/1/*".to_string(),
            args: vec![OscType::Int(1)],
        });
        group.throughput(Throughput::Elements(*size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &_size| {
            b.iter(|| server.handle_packet(packet.clone(), SocketAddr::V4(addr)));
        });
    }
    group.finish();
}

fn bench(c: &mut Criterion) {
    bench_handler_dispatch(c);
    bench_handler_dispatch_wildcard(c);
}

criterion_group!(benches, bench);
criterion_main!(benches);
