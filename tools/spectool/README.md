# Spec tool

# Compile the spectool

```bash
# from a top-level workspace directory (aka `polkavm`)
cargo build --release -p spectool
```

## How to assemble a single test case?

``` bash
./target/release/spectool prepare ./path/to/testcase.txt

# example fibonacci
./target/release/spectool prepare ./tools/spectool/spec/src/fib.txt > fib.json
```

The command above will print out a JSON test file content that can be piped to
a file.
