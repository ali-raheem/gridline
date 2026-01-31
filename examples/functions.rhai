// Example custom functions for Gridline
// Load with: gridline -f examples/functions.rhai
// Or at runtime: :source examples/functions.rhai
//
// Built-in functions (ALLCAPS):
//   SUM, AVG, COUNT, MIN, MAX  - range aggregations
//   VEC                        - range to array
//   BARCHART, LINECHART, SCATTER - plots
//   SPILL                      - convert range to array, e.g. (0..=10).SPILL()
//
// Rhai array methods you can use:
//   .order()      - returns sorted copy (ascending)
//   .order_desc() - returns sorted copy (descending)
//   .filter(|x| ...) - filter elements
//   .map(|x| ...)    - transform elements
//
// Examples:
//   =VEC(A1:A10).order().SPILL()
//   =VEC(A1:A10).filter(|x| x > 5).SPILL()
//   =(1..=10).map(|x| x * x).SPILL()

// Simple utility functions
fn square(x) { x * x }
fn cube(x) { x * x * x }
fn double(x) { x * 2 }
fn half(x) { x / 2 }

// Mathematical functions
fn abs(x) { if x < 0 { -x } else { x } }
fn sign(x) { if x < 0 { -1 } else if x > 0 { 1 } else { 0 } }
fn clamp(x, min, max) { if x < min { min } else if x > max { max } else { x } }
fn fib(n) { if n < 2 { n } else { fib(n-1) + fib(n-2) } }

// Financial functions
fn pmt(rate, periods, principal) {
    // Monthly payment for a loan
    if rate == 0 {
        principal / periods
    } else {
        let r = rate / 12.0;
        let n = periods;
        principal * r * (1 + r).pow(n) / ((1 + r).pow(n) - 1)
    }
}

fn compound_interest(principal, rate, years) {
    principal * (1 + rate).pow(years)
}

fn simple_interest(principal, rate, years) {
    principal * (1 + rate * years)
}

// Percentage functions
fn percent(value, total) {
    if total == 0 { 0 } else { (value / total) * 100 }
}

fn percent_of(percent, value) {
    value * percent / 100
}

fn percent_change(ov, nv) {
    if ov == 0 { 0 } else { ((nv - ov) / ov) * 100 }
}

// Unit conversions
fn celsius_to_fahrenheit(c) { c * 9.0 / 5.0 + 32 }
fn fahrenheit_to_celsius(f) { (f - 32) * 5.0 / 9.0 }
fn km_to_miles(km) { km * 0.621371 }
fn miles_to_km(miles) { miles * 1.60934 }

// Rounding functions
fn round_to(x, decimals) {
    let factor = 10.0.pow(decimals);
    (x * factor + 0.5).floor() / factor
}

