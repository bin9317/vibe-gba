use gba_core::bus::Bus;

#[test]
fn test_print_sizes() {
    println!("Bus size: {} bytes", std::mem::size_of::<Bus>());
    println!(
        "Ppu size: {} bytes",
        std::mem::size_of::<gba_core::ppu::Ppu>()
    );
}
