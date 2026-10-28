//! Native host entry for the web example.
//!
//! `run_dom_a11y_demo` is an explicit retained web/Engine support check; the
//! ordinary counter itself is built by the facade in `run_web_counter`.

fn main() {
    let n = web_hello::run_dom_a11y_demo();
    println!("web_hello: synced {n} a11y nodes");
    println!("web_hello: launching facade counter (desktop host)");
    web_hello::run_web_counter();
}
