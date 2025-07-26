use harness_config::resolver::{find_variables, parse_variable};

fn main() {
    let test_cases = vec![
        "${postgres.invalid_property}",
        "${postgres.ip}",
        "${postgres.port}",
        "${postgres.host}",
        "${API_HOST:-0.0.0.0}",
        "${missing_service.ip}",
    ];

    for test in test_cases {
        println!("\nTesting: {}", test);
        let vars = find_variables(test);
        for var_result in vars {
            match var_result {
                Ok((start, end, var)) => {
                    println!("  Found at {}..{}: {:?}", start, end, var);
                    let expr = &test[start + 2..end - 1]; // Extract the expression
                    println!("  Expression: '{}'", expr);
                }
                Err(e) => {
                    println!("  Parse error: {}", e);
                }
            }
        }
    }
}
