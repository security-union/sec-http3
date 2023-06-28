
#!/bin/bash

# Generates a certificate for local host
# Output:
# localhost.crt
# localhost.key

set -e
# generate certificate
openssl req -x509 -newkey rsa:2048 -keyout localhost-pem.key -out localhost.pem -days 365 -nodes -subj "/CN=127.0.0.1"

# Convert to der form
openssl x509 -in localhost.pem -outform der -out localhost.crt

openssl rsa -in localhost-pem.key -outform DER -out localhost.key
