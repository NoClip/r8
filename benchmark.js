// Performance Benchmark: Pure Safe Rust V8 vs Upstream Google V8 (Node.js)
// Compatible with both V8 d8 shell and Node.js runtime

var log = console.log;
if (typeof log !== "function") {
    log = print;
}

function runBenchmark(name, fn) {
    var start = Date.now();
    var result = fn();
    var end = Date.now();
    var duration = end - start;
    if (duration === 0) duration = 1; // prevent div by zero
    log(name + ": " + duration + " ms (result: " + result + ")");
    return duration;
}

var platformName = "R8 (Rust V8) d8 Shell";
if (typeof process !== "undefined") {
    platformName = "Node.js " + process.version + " (V8 " + process.versions.v8 + ")";
}
log("==================================================================");
log("  V8 Engine Performance Benchmark Suite");
log("  Platform: " + platformName);
log("==================================================================");

// 1. Arithmetic & Loop Throughput
var t1 = runBenchmark("1. Loop & Arithmetic Throughput (500k ops)", function() {
    var sum = 0;
    for (var i = 0; i < 500000; i = i + 1) {
        sum = (sum + (i ^ 3) * 2) % 100000007;
    }
    return sum;
});

// 2. Recursive Fibonacci (Call Frame Allocation & Stack Depth)
function fib(n) {
    if (n <= 1) return n;
    return fib(n - 1) + fib(n - 2);
}

var t2 = runBenchmark("2. Recursive Fibonacci fib(26)", function() {
    return fib(26);
});

// 3. Object Creation & Field Access (Hidden Class / Inline Cache)
var t3 = runBenchmark("3. Object Creation & IC Access (20k objs)", function() {
    var total = 0;
    for (var i = 0; i < 20000; i = i + 1) {
        var obj = { x: i, y: i * 2, sum: 0 };
        obj.sum = obj.x + obj.y;
        total = (total + obj.sum) % 100000007;
    }
    return total;
});

// 4. TypedArray Throughput (Int32Array Read/Write)
var t4 = runBenchmark("4. TypedArray Int32Array (20k elements)", function() {
    var ta = new Int32Array(20000);
    for (var i = 0; i < 20000; i = i + 1) {
        ta[i] = (i * 7) & 0xffff;
    }
    var sum = 0;
    for (var j = 0; j < 20000; j = j + 1) {
        sum = (sum + ta[j]) % 100000007;
    }
    return sum;
});

// 5. Array Push & Array Iteration
var t5 = runBenchmark("5. Array Push & Iteration (20k elements)", function() {
    var arr = [];
    for (var i = 0; i < 20000; i = i + 1) {
        arr.push(i);
    }
    var sum = 0;
    for (var j = 0; j < arr.length; j = j + 1) {
        sum = (sum + arr[j]) % 100000007;
    }
    return sum;
});

// 6. String Building & Slicing
var t6 = runBenchmark("6. String Concatenation & Slicing (10k ops)", function() {
    var s = "abcdefghijklmnopqrstuvwxyz";
    var result = "";
    for (var i = 0; i < 500; i = i + 1) {
        result = result + s.substring(i % 10, (i % 10) + 5);
    }
    return result.length;
});

var totalTime = t1 + t2 + t3 + t4 + t5 + t6;
log("------------------------------------------------------------------");
log("Total Benchmark Suite Execution Time: " + totalTime + " ms");
log("==================================================================");
