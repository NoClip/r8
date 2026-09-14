import subprocess
import re
import sys
import json
import statistics
import os

SCRIPT = "benchmark.js"
RUNS = 3

TARGETS = [
    {
        "name": "Rust V8 (Pure Safe Rust)",
        "cmd": ["target\\release\\d8.exe", SCRIPT],
        "category": "Rust V8"
    },
    {
        "name": "Google V8 (JIT-less Interpreter)",
        "cmd": ["node", "--jitless", SCRIPT],
        "category": "Upstream V8"
    },
    {
        "name": "Google V8 (Full JIT - TurboFan)",
        "cmd": ["node", SCRIPT],
        "category": "Upstream V8"
    }
]

def run_target(target):
    times = []
    bench_data = {}
    print(f"\n[Running] {target['name']} ({RUNS} iterations)...")
    
    for r in range(RUNS):
        res = subprocess.run(target["cmd"], capture_output=True, text=True)
        if res.returncode != 0:
            print(f"Error running {target['name']}: {res.stderr}")
            return None
        
        output = res.stdout
        lines = output.strip().split("\n")
        
        for line in lines:
            # Pattern: 1. Loop & Arithmetic Throughput (500k ops): 236 ms (result: 99482507)
            match = re.match(r"^(\d+\..*?):\s+(\d+)\s+ms\s+\(result:\s+(.*?)\)", line.strip())
            if match:
                bench_name = match.group(1).strip()
                ms = int(match.group(2))
                result_val = match.group(3).strip()
                
                if bench_name not in bench_data:
                    bench_data[bench_name] = {"times": [], "results": []}
                bench_data[bench_name]["times"].append(ms)
                bench_data[bench_name]["results"].append(result_val)
        
        # Total line: Total Benchmark Suite Execution Time: 525 ms
        total_match = re.search(r"Total Benchmark Suite Execution Time:\s+(\d+)\s+ms", output)
        if total_match:
            times.append(int(total_match.group(1)))
    
    summary = {}
    for bench_name, data in bench_data.items():
        avg = statistics.mean(data["times"])
        std = statistics.stdev(data["times"]) if len(data["times"]) > 1 else 0
        summary[bench_name] = {
            "avg_ms": round(avg, 1),
            "min_ms": min(data["times"]),
            "max_ms": max(data["times"]),
            "std_ms": round(std, 1),
            "result": data["results"][0]
        }
    
    return {
        "name": target["name"],
        "category": target["category"],
        "benchmarks": summary,
        "total_avg_ms": round(statistics.mean(times), 1) if times else 0,
        "total_min_ms": min(times) if times else 0
    }

def main():
    results = []
    for t in TARGETS:
        res = run_target(t)
        if res:
            results.append(res)
    
    print("\n" + "="*80)
    print("  GOOGLE V8 (C++) vs RUST V8 PERFORMANCE BENCHMARK COMPARISON")
    print("="*80)
    print(f"{'Benchmark':<45} | {'Rust V8':<10} | {'V8 (Jitless)':<13} | {'V8 (Full JIT)':<13}")
    print("-" * 89)
    
    bench_names = list(results[0]["benchmarks"].keys())
    for b in bench_names:
        r_ms = results[0]["benchmarks"].get(b, {}).get("avg_ms", 0)
        v_jitless = results[1]["benchmarks"].get(b, {}).get("avg_ms", 0)
        v_jit = results[2]["benchmarks"].get(b, {}).get("avg_ms", 0)
        print(f"{b:<45} | {r_ms:>7.1f} ms | {v_jitless:>10.1f} ms | {v_jit:>10.1f} ms")
    
    print("-" * 89)
    t_rust = results[0]["total_avg_ms"]
    t_jitless = results[1]["total_avg_ms"]
    t_jit = results[2]["total_avg_ms"]
    print(f"{'Total Benchmark Suite Execution Time':<45} | {t_rust:>7.1f} ms | {t_jitless:>10.1f} ms | {t_jit:>10.1f} ms")
    print("="*89)
    
    # Correctness Verification
    print("\n[Correctness Verification]")
    all_matched = True
    for b in bench_names:
        res_rust = results[0]["benchmarks"][b]["result"]
        res_v8 = results[2]["benchmarks"][b]["result"]
        matched = (res_rust == res_v8)
        status = "MATCH (100% IDENTICAL)" if matched else f"MISMATCH ({res_rust} vs {res_v8})"
        print(f"  {b:<42} : {status}")
        if not matched:
            all_matched = False
            
    print(f"\nAll Benchmark Calculations Match Official V8 Exactly: {'YES (100% PASS)' if all_matched else 'NO'}")
    
    # Save results as JSON
    with open("benchmark_results.json", "w") as f:
        json.dump(results, f, indent=2)
    print("\nSaved detailed benchmark data to benchmark_results.json")

if __name__ == "__main__":
    main()
