use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

const ITERATIONS: usize = 1_000_000;
const THREADED_REQUESTS: usize = 100_000;
const MAX_CONCURRENT_THREADS: usize = 4_000;
const KEY_PREFIX: &str = "abcdefghijklmno";
const NUM_KEYS: u32 = 100;

fn xorshift64(state: &mut u64) -> u64 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    *state = x;
    x
}

fn make_keys() -> Vec<String> {
    (0..NUM_KEYS)
        .map(|i| format!("{}{}", KEY_PREFIX, i))
        .collect()
}

fn make_map() -> Mutex<HashMap<String, u64>> {
    let map = Mutex::new(HashMap::new());
    {
        let mut guard = map.lock().unwrap();
        for i in 0..NUM_KEYS {
            guard.insert(format!("{}{}", KEY_PREFIX, i), 0);
        }
    }
    map
}

fn seed_rng() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64
        | 1
}

fn run_single_threaded() {
    let map = make_map();
    let keys = make_keys();

    let mut rng_state = seed_rng();
    let access_indices: Vec<usize> = (0..ITERATIONS)
        .map(|_| (xorshift64(&mut rng_state) % NUM_KEYS as u64) as usize)
        .collect();

    let mut samples_ns: Vec<u128> = Vec::with_capacity(ITERATIONS);

    for i in 0..ITERATIONS {
        let key = &keys[access_indices[i]];
        let start = Instant::now();
        {
            let mut guard = map.lock().unwrap();
            let entry = guard.get_mut(key).unwrap();
            *entry += 1;
        }
        samples_ns.push(start.elapsed().as_nanos());
    }

    samples_ns.sort_unstable();
    let total: u128 = samples_ns.iter().sum();
    let mean = total as f64 / samples_ns.len() as f64;
    let min = *samples_ns.first().unwrap();
    let max = *samples_ns.last().unwrap();
    let p50 = samples_ns[samples_ns.len() / 2];
    let p95 = samples_ns[(samples_ns.len() as f64 * 0.95) as usize];
    let p99 = samples_ns[(samples_ns.len() as f64 * 0.99) as usize];

    let final_total: u64 = map.lock().unwrap().values().sum();

    println!("=== Benchmark 1: single-threaded lock+update+unlock ===");
    println!("  iterations:   {}", ITERATIONS);
    println!("  num keys:     {}", NUM_KEYS);
    println!("  sum of vals:  {}", final_total);
    println!("  total time:   {:.3} ms", total as f64 / 1_000_000.0);
    println!("  mean:         {:.2} ns", mean);
    println!("  min:          {} ns", min);
    println!("  p50:          {} ns", p50);
    println!("  p95:          {} ns", p95);
    println!("  p99:          {} ns", p99);
    println!("  max:          {} ns", max);
}

fn run_threaded() {
    let map = Arc::new(make_map());
    let keys = Arc::new(make_keys());

    let mut rng_state = seed_rng();
    let access_indices: Vec<usize> = (0..THREADED_REQUESTS)
        .map(|_| (xorshift64(&mut rng_state) % NUM_KEYS as u64) as usize)
        .collect();

    let mut handles: Vec<thread::JoinHandle<u128>> = Vec::with_capacity(MAX_CONCURRENT_THREADS);
    let mut samples_ns: Vec<u128> = Vec::with_capacity(THREADED_REQUESTS);

    let start = Instant::now();
    for idx in access_indices {
        if handles.len() >= MAX_CONCURRENT_THREADS {
            samples_ns.push(handles.remove(0).join().unwrap());
        }
        let map = Arc::clone(&map);
        let keys = Arc::clone(&keys);
        let handle = thread::Builder::new()
            .stack_size(64 * 1024)
            .spawn(move || {
                let key = &keys[idx];
                let t = Instant::now();
                {
                    let mut guard = map.lock().unwrap();
                    let entry = guard.get_mut(key).unwrap();
                    *entry += 1;
                }
                t.elapsed().as_nanos()
            })
            .expect("failed to spawn thread");
        handles.push(handle);
    }
    for h in handles {
        samples_ns.push(h.join().unwrap());
    }
    let elapsed = start.elapsed();

    samples_ns.sort_unstable();
    let final_total: u64 = map.lock().unwrap().values().sum();
    let total_ns = elapsed.as_nanos();
    let avg_ns = total_ns as f64 / THREADED_REQUESTS as f64;
    let sum_ns: u128 = samples_ns.iter().sum();
    let mean = sum_ns as f64 / samples_ns.len() as f64;
    let min = *samples_ns.first().unwrap();
    let max = *samples_ns.last().unwrap();
    let p50 = samples_ns[samples_ns.len() / 2];
    let p95 = samples_ns[(samples_ns.len() as f64 * 0.95) as usize];
    let p99 = samples_ns[(samples_ns.len() as f64 * 0.99) as usize];

    println!();
    println!("=== Benchmark 2: {}-threaded lock+update+unlock ===", THREADED_REQUESTS);
    println!("  threads:      {} (one per request)", THREADED_REQUESTS);
    println!("  max in-flight:{}", MAX_CONCURRENT_THREADS);
    println!("  num keys:     {}", NUM_KEYS);
    println!("  sum of vals:  {}", final_total);
    println!("  total time:   {:.3} ms", total_ns as f64 / 1_000_000.0);
    println!("  avg/request:  {:.2} ns", avg_ns);
    println!("  mean:         {:.2} ns", mean);
    println!("  min:          {} ns", min);
    println!("  p50:          {} ns", p50);
    println!("  p95:          {} ns", p95);
    println!("  p99:          {} ns", p99);
    println!("  max:          {} ns", max);
}

fn main() {
    assert_eq!(KEY_PREFIX.len(), 15);
    run_single_threaded();
    run_threaded();
}
