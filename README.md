# php-skywalking-agent

Non intrusive PHP skywalking agent, written in Rust.

*Only support on Linux now.*

NOTE: This is a php extension project, which use pure Rust rather than C/C++, power by [phper](https://github.com/jmjoy/phper).

## How to use?

1. Install [Rust](https://www.rust-lang.org/).

    See: <https://www.rust-lang.org/tools/install>.

1. Clone the repository.

   ```shell
   git clone --recursive https://github.com/jmjoy/php-skywalking-agent.git
   cd php-skywalking-agent
   ```

1. Build and install the extension.

    ```shell
    # Optional, specify if php isn't installed globally.
    # export PHP_CONFIG=<Your path of php-config>

    # Build libskywalking_agent.so.
    cargo build --release

    ./target/release/skywalking_agent install
    ```

1. Configure the php.ini.

    ```ini
    extension = skywalking_agent
    ```

## License

MulanPSL-2.0.
