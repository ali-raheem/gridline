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

fn affine(v, mini, maxi, mino, maxo) {
    let v = v.to_float();
    let in_min = mini.to_float();
    let in_max = maxi.to_float();
    let out_min = mino.to_float();
    let out_max = maxo.to_float();

    let t = (v - in_min) / (in_max - in_min);

    out_min + (t * (out_max - out_min))
}

// Financial functions
fn pmt(rate, periods, principal) {
    // Payment for a loan (matches Excel/Sheets PMT)
    // rate = periodic interest rate (e.g., annual_rate/12 for monthly)
    // periods = total number of payments
    // principal = loan amount (present value)
    if rate == 0 {
        principal / periods
    } else {
        principal * rate * POW(1 + rate, periods) / (POW(1 + rate, periods) - 1)
    }
}

fn compound_interest(principal, rate, years) {
    principal * POW(1 + rate, years)
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
    let factor = POW(10.0, decimals);
    (x * factor + 0.5).floor() / factor
}

// ============================================================================
// Statistical Functions
// ============================================================================

fn stdev(arr) {
    // Standard deviation (sample) - matches Excel/LibreOffice STDEV
    let n = arr.len();
    if n < 2 { return 0.0; }
    let mean = 0.0;
    for x in arr { mean += x; }
    mean = mean / n;
    let sum_sq = 0.0;
    for x in arr { sum_sq += (x - mean) * (x - mean); }
    (sum_sq / (n - 1)).sqrt()
}

fn stdevp(arr) {
    // Standard deviation (population)
    let n = arr.len();
    if n == 0 { return 0.0; }
    let mean = 0.0;
    for x in arr { mean += x; }
    mean = mean / n;
    let sum_sq = 0.0;
    for x in arr { sum_sq += (x - mean) * (x - mean); }
    (sum_sq / n).sqrt()
}

fn variance(arr) {
    // Variance (sample) - matches Excel/LibreOffice VAR
    let n = arr.len();
    if n < 2 { return 0.0; }
    let mean = 0.0;
    for x in arr { mean += x; }
    mean = mean / n;
    let sum_sq = 0.0;
    for x in arr { sum_sq += (x - mean) * (x - mean); }
    sum_sq / (n - 1)
}

fn variancep(arr) {
    // Variance (population)
    let n = arr.len();
    if n == 0 { return 0.0; }
    let mean = 0.0;
    for x in arr { mean += x; }
    mean = mean / n;
    let sum_sq = 0.0;
    for x in arr { sum_sq += (x - mean) * (x - mean); }
    sum_sq / n
}

fn median(arr) {
    // Middle value of sorted array
    let n = arr.len();
    if n == 0 { return 0.0; }
    let sorted = arr.order();
    if n % 2 == 1 {
        sorted[n / 2]
    } else {
        (sorted[n / 2 - 1] + sorted[n / 2]) / 2.0
    }
}

fn mode(arr) {
    // Most frequent value (returns first if tie)
    if arr.len() == 0 { return 0; }
    let counts = #{};
    let max_count = 0;
    let mode_val = arr[0];
    for x in arr {
        let key = `${x}`;
        if counts.contains(key) {
            counts[key] += 1;
        } else {
            counts[key] = 1;
        }
        if counts[key] > max_count {
            max_count = counts[key];
            mode_val = x;
        }
    }
    mode_val
}

fn product(arr) {
    // Multiply all elements
    let result = 1;
    for x in arr { result *= x; }
    result
}

fn geomean(arr) {
    // Geometric mean
    let n = arr.len();
    if n == 0 { return 0.0; }
    let prod = 1.0;
    for x in arr { prod *= x; }
    POW(prod, 1.0 / n)
}

// ============================================================================
// Array Utilities
// ============================================================================

fn unique(arr) {
    // Remove duplicates (preserves order)
    let seen = #{};
    let result = [];
    for x in arr {
        let key = `${x}`;
        if !seen.contains(key) {
            seen[key] = true;
            result.push(x);
        }
    }
    result
}

fn reverse(arr) {
    // Reverse order
    let result = [];
    let i = arr.len();
    while i > 0 {
        i -= 1;
        result.push(arr[i]);
    }
    result
}

fn take(arr, n) {
    // First n elements
    let result = [];
    let count = if n < arr.len() { n } else { arr.len() };
    for i in 0..count {
        result.push(arr[i]);
    }
    result
}

fn skip(arr, n) {
    // Skip first n elements
    let result = [];
    let start = if n > arr.len() { arr.len() } else { n };
    for i in start..arr.len() {
        result.push(arr[i]);
    }
    result
}

fn running_sum(arr) {
    // Cumulative sum: [1,2,3] -> [1,3,6]
    let result = [];
    let sum = 0;
    for x in arr {
        sum += x;
        result.push(sum);
    }
    result
}

fn running_avg(arr) {
    // Cumulative average: [1,2,3] -> [1,1.5,2]
    let result = [];
    let sum = 0.0;
    let count = 0;
    for x in arr {
        sum += x;
        count += 1;
        result.push(sum / count);
    }
    result
}

fn zip(a, b) {
    // Pair elements: [[a1,b1], [a2,b2], ...]
    let result = [];
    let len = if a.len() < b.len() { a.len() } else { b.len() };
    for i in 0..len {
        result.push([a[i], b[i]]);
    }
    result
}

fn enumerate(arr) {
    // Add indices: [[0,a], [1,b], ...]
    let result = [];
    for i in 0..arr.len() {
        result.push([i, arr[i]]);
    }
    result
}

fn range_arr(start, end, step) {
    // Generate sequence from start to end by step
    let result = [];
    let val = start;
    if step > 0 {
        while val <= end {
            result.push(val);
            val += step;
        }
    } else if step < 0 {
        while val >= end {
            result.push(val);
            val += step;
        }
    }
    result
}

// ============================================================================
// Text Helpers
// ============================================================================

fn concat_arr(arr, sep) {
    // Join array with separator
    let result = "";
    for i in 0..arr.len() {
        if i > 0 { result += sep; }
        result += `${arr[i]}`;
    }
    result
}

fn left(s, n) {
    // First n characters
    s.sub_string(0, n)
}

fn right(s, n) {
    // Last n characters
    let len = s.len();
    if n >= len { s } else { s.sub_string(len - n) }
}

fn mid(s, start, len) {
    // Substring from start with length
    s.sub_string(start, len)
}

fn pad_left(s, n, ch) {
    // Left pad to n chars with ch
    let result = s;
    while result.len() < n {
        result = ch + result;
    }
    result
}

fn pad_right(s, n, ch) {
    // Right pad to n chars with ch
    let result = s;
    while result.len() < n {
        result += ch;
    }
    result
}

fn repeat_str(s, n) {
    // Repeat string n times
    let result = "";
    for i in 0..n {
        result += s;
    }
    result
}

// ============================================================================
// Lookup/Search Functions
// ============================================================================

fn index_of(arr, val) {
    // Find position (-1 if not found)
    for i in 0..arr.len() {
        if arr[i] == val { return i; }
    }
    -1
}

fn contains(arr, val) {
    // Boolean check if value exists
    for x in arr {
        if x == val { return true; }
    }
    false
}

fn count_val(arr, val) {
    // Count occurrences of value
    let count = 0;
    for x in arr {
        if x == val { count += 1; }
    }
    count
}

fn max_index(arr) {
    // Index of max value
    if arr.len() == 0 { return -1; }
    let max_i = 0;
    let max_v = arr[0];
    for i in 1..arr.len() {
        if arr[i] > max_v {
            max_v = arr[i];
            max_i = i;
        }
    }
    max_i
}

fn min_index(arr) {
    // Index of min value
    if arr.len() == 0 { return -1; }
    let min_i = 0;
    let min_v = arr[0];
    for i in 1..arr.len() {
        if arr[i] < min_v {
            min_v = arr[i];
            min_i = i;
        }
    }
    min_i
}

// ============================================================================
// Financial Functions (Additional)
// ============================================================================

fn fv(rate, periods, pmt) {
    // Future value of periodic payments
    if rate == 0 {
        pmt * periods
    } else {
        pmt * (POW(1 + rate, periods) - 1) / rate
    }
}

fn pv(rate, periods, fv_val) {
    // Present value of future amount
    fv_val / POW(1 + rate, periods)
}

fn npv(rate, cashflows) {
    // Net present value of cash flows
    let result = 0.0;
    for i in 0..cashflows.len() {
        result += cashflows[i] / POW(1 + rate, i + 1);
    }
    result
}

fn cagr(start_val, end_val, years) {
    // Compound annual growth rate
    if start_val == 0 || years == 0 { return 0.0; }
    POW(end_val / start_val, 1.0 / years) - 1
}

// ============================================================================
// Logic Helpers
// ============================================================================

fn iff(cond, t, f) {
    // Ternary helper (readable in formulas)
    if cond { t } else { f }
}

fn and_all(arr) {
    // All true?
    for x in arr {
        if !x { return false; }
    }
    true
}

fn or_any(arr) {
    // Any true?
    for x in arr {
        if x { return true; }
    }
    false
}

fn none(arr) {
    // All false?
    for x in arr {
        if x { return false; }
    }
    true
}

fn coalesce(a, b) {
    // Return a if truthy, else b
    if a { a } else { b }
}

// ============================================================================
// Date/Formatting Functions
// ============================================================================

fn format_currency(n, decimals) {
    // Round to specified decimals: 1234.567 -> "1234.57"
    let factor = POW(10.0, decimals);
    let rounded = ((n * factor) + 0.5).floor() / factor;
    `${rounded}`
}

fn add_commas(s) {
    // Add thousand separators to a number string
    let result = "";
    let len = s.len();
    for i in 0..len {
        let pos = len - i;
        if i > 0 && pos % 3 == 0 { result += ","; }
        result += s.sub_string(i, 1);
    }
    result
}

fn format_pct(n, decimals) {
    // Format as percentage: 0.123 -> "12.3%"
    let factor = POW(10.0, decimals);
    let val = ((n * 100 * factor) + 0.5).floor() / factor;
    `${val}%`
}

fn format_percent(n, decimals) {
    // Alias for format_pct
    format_pct(n, decimals)
}

