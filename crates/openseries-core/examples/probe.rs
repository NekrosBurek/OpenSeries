//! `cargo run -p openseries-core --example probe`: print what the agent would see.
fn main() {
    let mut api = hidapi::HidApi::new().expect("hidapi");
    for f in openseries_core::devices::scan(&mut api) {
        let d = openseries_core::devices::read(&api, &f);
        println!("{:04x} {:?} {:?} {:?} {:?} {:?} raw={:02x?}", d.pid, d.name, d.kind, d.link, d.battery, d.chatmix, &d.raw[..d.raw.len().min(8)]);
    }
    #[cfg(windows)]
    {
        use openseries_core::audio::*;
        init_com();
        for flow in [Flow::Output, Flow::Input] {
            println!("{flow:?}: {:?}", endpoints(flow).iter().map(|e| &e.name).collect::<Vec<_>>());
            println!("  default comms: {:?}", default_id(flow, ROLES[2]));
        }
    }
}
