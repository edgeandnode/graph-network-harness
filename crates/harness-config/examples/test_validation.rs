use harness_config::{parser, resolver};

fn main() {
    println!("Test 1: Invalid service reference");
    let yaml1 = r#"
version: "1.0"
networks:
  local:
    type: local
services:
  api:
    type: process
    network: local
    binary: api-server
    env:
      DATABASE_URL: "postgresql://user:pass@${missing_service.ip}:5432/db"
"#;

    let result1 = parser::parse_str(yaml1);
    match result1 {
        Ok(_) => println!("  Unexpectedly succeeded"),
        Err(e) => println!("  Error (expected): {}", e),
    }
    
    println!("\nTest 2: Invalid property");
    let yaml2 = r#"
version: "1.0"
networks:
  local:
    type: local
services:
  postgres:
    type: docker
    network: local
    image: postgres
  api:
    type: process
    network: local
    binary: api-server
    env:
      INVALID_REF: "${postgres.invalid_property}"
"#;

    let result2 = parser::parse_str(yaml2);
    match result2 {
        Ok(config) => {
            println!("  Parsed successfully");
            let (env_vars, service_refs) = resolver::find_all_references(&config).unwrap();
            println!("  Env vars: {:?}", env_vars);
            println!("  Service refs: {:?}", service_refs);
        }
        Err(e) => println!("  Error: {}", e),
    }
}