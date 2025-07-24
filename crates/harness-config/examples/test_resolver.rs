use harness_config::{parser, resolver};

fn main() {
    let yaml = r#"
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

    // The parser now validates references during parsing
    let result = parser::parse_str(yaml);
    match result {
        Ok(config) => {
            println!("Config parsed successfully (unexpected!)");
            let (env_vars, service_refs) = resolver::find_all_references(&config).unwrap();
            println!("Env vars: {:?}", env_vars);
            println!("Service refs: {:?}", service_refs);
        }
        Err(e) => {
            println!("Parse error (expected): {}", e);
        }
    }
}