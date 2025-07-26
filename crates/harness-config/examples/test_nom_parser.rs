fn main() {
    println!("Testing basic nom parsing:");

    use nom::{bytes::complete::tag, sequence::delimited, IResult};

    fn simple_parser(input: &str) -> IResult<&str, &str> {
        delimited(tag(r#"${"#), nom::combinator::rest, tag(r#"}"#))(input)
    }

    let test_input = r#"${TEST_VAR}"#;
    match simple_parser(test_input) {
        Ok((rest, content)) => println!(
            "Simple parser works: content='{}', rest='{}'",
            content, rest
        ),
        Err(e) => println!("Simple parser failed: {:?}", e),
    }

    // Test our actual parser components
    use harness_config::resolver::parse_variable;

    let test_cases = vec![
        r#"${TEST_VAR}"#,
        r#"${TEST123}"#,
        r#"${postgres.ip}"#,
        r#"${test_var}"#, // Should fail - lowercase
        r#"${Test_Var}"#, // Should fail - mixed case
        r#"${123TEST}"#,  // Should fail - starts with digit
    ];

    for test in test_cases {
        println!("\nTesting full input '{}':", test);
        match parse_variable(test) {
            Ok((rest, result)) => {
                println!("  Rest: '{}'", rest);
                match result {
                    Ok(var) => println!("  Success: {:?}", var),
                    Err(e) => println!("  Parse error: {}", e),
                }
            }
            Err(e) => println!("  Nom error: {:?}", e),
        }
    }
}
