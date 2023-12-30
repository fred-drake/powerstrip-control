use powerstrip_control::SmartPowerStrip;

fn main() {
    let s = SmartPowerStrip::new("192.168.40.93".to_string(), None, None, None);
    println!("{:?}", s.get_system_info());

    s.toggle_plug("Plug 5", powerstrip_control::PlugState::On)
        .unwrap();

    println!("{:?}", s.get_system_info());
}
