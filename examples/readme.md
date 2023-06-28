# Getting started

This is a quick starting guide to run the WebTransport example.

## Generate the required certs/keys and launch chrome

```sh
cd examples
./generate_certs.sh
```

## Opening Chrome

If you are using a self-signed certificate (as per above) you need to explicitly tel Chrome to trust it.

The following script launches Chrome by trusing `localhost.crt`

**Note**: If you are on Mac you will need to `Quit` or else it will just open a new window in the existing instance
without the supplied command line argument.

```sh
cd examples
./launch_chrome.sh
```

## Run the Server

Withing the root, run:

```sh
RUST_LOG="debug" cargo run --example webtransport_server
```

The server will by default listen on `127.0.0.1:4433`.

**Note**: Chrome will only accept the generated keys for `127.0.0.1:4433` so connecting to `localhost:4433` won't work.

Head to <https://security-union.github.io/yew-webtransport/>

Change the server endpoint to `https://127.0.0.1:4433` try to send datagrams, the sample server should echo them to the client.
