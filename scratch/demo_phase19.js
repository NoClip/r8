// Phase 19 Demo: WebAssembly Baseline Engine (Liftoff)
// Run: d8 scratch/demo_phase19.js

console.log("=== Phase 19: WebAssembly Baseline Engine ===\n");

// 1. Inspect the WebAssembly global object
if (typeof WebAssembly === "object") {
    console.log("WebAssembly global: OK");
    console.log("  .validate:    " + (typeof WebAssembly.validate));
    console.log("  .compile:     " + (typeof WebAssembly.compile));
    console.log("  .instantiate: " + (typeof WebAssembly.instantiate));
}

// 2. Hand-assembled add(i32, i32) -> i32
var addWasmBytes = [
    0x00, 0x61, 0x73, 0x6D,  // magic "\0asm"
    0x01, 0x00, 0x00, 0x00,  // version 1
    // type section: (i32, i32) -> i32
    0x01, 0x07, 0x01, 0x60, 0x02, 0x7F, 0x7F, 0x01, 0x7F,
    // function section: func[0] uses type[0]
    0x03, 0x02, 0x01, 0x00,
    // export section: "add" -> func[0]
    0x07, 0x07, 0x01, 0x03, 0x61, 0x64, 0x64, 0x00, 0x00,
    // code section: local.get 0, local.get 1, i32.add, end
    0x0A, 0x09, 0x01, 0x07, 0x00, 0x20, 0x00, 0x20, 0x01, 0x6A, 0x0B
];

console.log("\n--- WebAssembly.validate ---");
console.log("validate(addWasmBytes) = " + WebAssembly.validate(addWasmBytes)); // true

console.log("\n--- WebAssembly.instantiate ---");
var result = WebAssembly.instantiate(addWasmBytes);
var addFn = result.instance.exports.add;
console.log("typeof exports.add = " + typeof addFn);

console.log("\n--- Calling exported Wasm add ---");
console.log("add(10, 32)   = " + addFn(10, 32));    // 42
console.log("add(100, 200) = " + addFn(100, 200));   // 300
console.log("add(-5, 5)    = " + addFn(-5, 5));      // 0

// 3. mul_add(a, b, c) = (a * b) + c
// Matches make_mul_add_wasm() from differential tests exactly:
// [0, 97, 115, 109, 1, 0, 0, 0, 1, 8, 1, 96, 3, 127, 127, 127, 1, 127, 3, 2, 1, 0, 7, 11, 1, 7, 109, 117, 108, 95, 97, 100, 100, 0, 0, 10, 9, 1, 7, 0, 32, 0, 32, 1, 108, 32, 2, 106, 11]
var mulAddBytes = [
    0, 97, 115, 109, 1, 0, 0, 0,
    1, 8, 1, 96, 3, 127, 127, 127, 1, 127,
    3, 2, 1, 0,
    7, 11, 1, 7, 109, 117, 108, 95, 97, 100, 100, 0, 0,
    10, 12, 1, 10, 0, 32, 0, 32, 1, 108, 32, 2, 106, 11
];
var r2 = WebAssembly.instantiate(mulAddBytes);
var mulAdd = r2.instance.exports.mul_add;
console.log("\n--- mul_add(a, b, c) = a*b + c ---");
console.log("mul_add(3, 4, 5)    = " + mulAdd(3, 4, 5));     // 17
console.log("mul_add(10, 10, -1) = " + mulAdd(10, 10, -1));  // 99
console.log("mul_add(7, 6, 0)    = " + mulAdd(7, 6, 0));     // 42

// 4. JS still JIT-compiles alongside Wasm
console.log("\n--- JS (JIT) fibonacci ---");
function fib(n) {
    if (n <= 1) { return n; }
    return fib(n - 1) + fib(n - 2);
}
console.log("fib(10) = " + fib(10));  // 55
console.log("fib(15) = " + fib(15));  // 610

console.log("\n=== Phase 19 complete ===");
