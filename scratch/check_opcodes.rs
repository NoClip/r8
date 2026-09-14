use v8_base_bits::interpreter::bytecodes::Bytecode;
fn main() {
    println!("LdaGlobal = {}", Bytecode::LdaGlobal as u8);
    println!("SubSmi = {}", Bytecode::SubSmi as u8);
    println!("Star0 = {}", Bytecode::Star0 as u8);
    println!("Star1 = {}", Bytecode::Star1 as u8);
    println!("Star2 = {}", Bytecode::Star2 as u8);
    println!("Star3 = {}", Bytecode::Star3 as u8);
    println!("Ldar = {}", Bytecode::Ldar as u8);
    println!("CallUndefinedReceiver = {}", Bytecode::CallUndefinedReceiver as u8);
    println!("Add = {}", Bytecode::Add as u8);
    println!("Return = {}", Bytecode::Return as u8);
    println!("LdaSmi = {}", Bytecode::LdaSmi as u8);
    println!("TestLessThanOrEqual = {}", Bytecode::TestLessThanOrEqual as u8);
    println!("JumpIfFalse = {}", Bytecode::JumpIfFalse as u8);
}
