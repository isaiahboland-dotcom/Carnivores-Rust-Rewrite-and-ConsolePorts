//! Times what the 3DS menu does with a pack's `_RES.TXT`: parse, then one
//! resolve per area (`c2-3ds` `Rules::hunt`). `cargo run --release --example
//! res_cost -- _RES.TXT AREAS`.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().collect();
    let text = std::fs::read_to_string(&a[1])?;
    let areas: u32 = a.get(2).and_then(|n| n.parse().ok()).unwrap_or(1);
    let t = std::time::Instant::now();
    let raw = c2_formats::res_txt::ResTxt::parse(&text)?;
    println!("parse {:?}", t.elapsed());
    let t = std::time::Instant::now();
    for n in 1..=areas {
        let h = c2_formats::mee::Hunt {
            area: Some(n),
            time: 1,
            picked: c2_formats::bits::Bits::ALL,
        };
        let m = c2_core::mee::read(&raw, &h);
        std::hint::black_box(m);
    }
    println!("{areas} resolves {:?}", t.elapsed());
    Ok(())
}
