// Script helpers for :source + :call.
// These functions rely on script-only write builtins.
//
// Load in Gridline:
//   :source examples/script_ops.rhai
//
// Run:
//   :call seed_sales_table()
//   :call add_row("Notebook", 5, 3.50)
//   :call recalc_totals()
//   :call clear_demo()

fn seed_sales_table() {
    clear_demo();

    SET_CELL("A1", "Item");
    SET_CELL("B1", "Qty");
    SET_CELL("C1", "Price");
    SET_CELL("D1", "Total");

    SET_CELL("A2", "Pens");
    SET_CELL("B2", 10);
    SET_CELL("C2", 1.50);

    SET_CELL("A3", "Paper");
    SET_CELL("B3", 4);
    SET_CELL("C3", 5.25);

    recalc_totals();
}

fn add_row(item, qty, price) {
    let row = 2;
    while !ISEMPTY(VALUE(0, row)) {
        row += 1;
    }

    SET_CELL(0, row, item);
    SET_CELL(1, row, qty);
    SET_CELL(2, row, price);
    SET_CELL(3, row, qty * price);
}

fn recalc_totals() {
    let row = 2;
    while !ISEMPTY(VALUE(0, row)) {
        let qty = CELL(1, row);
        let price = CELL(2, row);
        SET_CELL(3, row, qty * price);
        row += 1;
    }

    let last = row - 1;
    let grand = 0.0;
    for r in 2..=last {
        grand += CELL(3, r);
    }
    SET_CELL("F1", "Grand Total");
    SET_CELL("G1", grand);
}

fn clear_demo() {
    CLEAR_RANGE(0, 0, 8, 40);
}
