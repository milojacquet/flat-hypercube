use flat_hypercube::state::main_inner;

fn main() {
    let res = main_inner();
    if let Err(err) = res {
        println!("{}", err);
    }
}
