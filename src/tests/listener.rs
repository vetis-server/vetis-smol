use crate::listener::tcp::TcpListener;
#[cfg(feature = "http3")]
use crate::listener::udp::UdpListener;
use std::net::{IpAddr, Ipv4Addr};
use vetis::listener::ListenerConfig;

fn listener(port: u16, interface: IpAddr) -> TcpListener {
    TcpListener::new(
        ListenerConfig::builder()
            .interface(interface)
            .port(port)
            .build()
            .unwrap(),
    )
}

#[cfg(feature = "http3")]
fn udp_listener(port: u16, interface: IpAddr) -> UdpListener {
    UdpListener::new(
        ListenerConfig::builder()
            .interface(interface)
            .port(port)
            .build()
            .unwrap(),
    )
}

#[test]
fn inequality_detects_port_or_interface_changes() {
    let original = listener(8080, Ipv4Addr::UNSPECIFIED.into());

    assert!(original != listener(8081, Ipv4Addr::UNSPECIFIED.into()));
    assert!(original != listener(8080, Ipv4Addr::LOCALHOST.into()));
}

#[cfg(feature = "http3")]
#[test]
fn udp_inequality_detects_port_or_interface_changes() {
    let original = udp_listener(8080, Ipv4Addr::UNSPECIFIED.into());

    assert!(original != udp_listener(8081, Ipv4Addr::UNSPECIFIED.into()));
    assert!(original != udp_listener(8080, Ipv4Addr::LOCALHOST.into()));
}
