# Test fixtures — TLS for the PostgreSQL client

**Test-only key material. Never trust this CA anywhere else.** The CA's private
key was discarded when these were generated, so nothing new can be signed by
it; the server keys below are public by virtue of being in this repo.

| File | What it is | Used by |
|---|---|---|
| `pg_tls_ca.pem` | The test CA (P-256, `CA:TRUE`) | `ca_file` for `pg_connect_tls` in both suites |
| `pg_tls_ec_server.der` / `.key.der` | A P-256 server leaf signed by it, DER (PKCS#8 key) | the in-process fake server in `tests/test_backends.tcyr` (lib/tls.cyr's native server presents ECDSA or Ed25519 only) |
| `pg_tls_rsa_server.pem` / `.key` | An RSA-2048 server leaf signed by it, PEM | the real PostgreSQL in CI's live job — and so the RSA path of the native client |

Both leaves carry `subjectAltName = DNS:majra-test.local, IP:127.0.0.1`,
`CA:FALSE`, `digitalSignature`, `serverAuth`, and expire in 2046 (UTCTime).

Regenerate (2.13.0's commands):

```bash
openssl req -x509 -newkey ec -pkeyopt ec_paramgen_curve:P-256 -nodes -keyout ca.key -out ca.pem -days 7300 -subj "/CN=majra test CA" -addext "basicConstraints=critical,CA:TRUE" -addext "keyUsage=critical,keyCertSign,cRLSign"
printf "subjectAltName=DNS:majra-test.local,IP:127.0.0.1\nbasicConstraints=critical,CA:FALSE\nkeyUsage=critical,digitalSignature,keyEncipherment\nextendedKeyUsage=serverAuth\n" > leaf.ext
openssl req -newkey ec -pkeyopt ec_paramgen_curve:P-256 -nodes -keyout ec.key -out ec.csr -subj "/CN=majra-test.local"
openssl x509 -req -in ec.csr -CA ca.pem -CAkey ca.key -CAcreateserial -out ec.pem -days 7300 -extfile leaf.ext
openssl req -newkey rsa:2048 -nodes -keyout rsa.key -out rsa.csr -subj "/CN=majra-test.local"
openssl x509 -req -in rsa.csr -CA ca.pem -CAkey ca.key -CAserial ca.srl -out rsa.pem -days 7300 -extfile leaf.ext
openssl x509 -in ec.pem -outform DER -out pg_tls_ec_server.der
openssl pkcs8 -topk8 -nocrypt -in ec.key -outform DER -out pg_tls_ec_server.key.der
# then: ca.pem -> pg_tls_ca.pem, rsa.pem / rsa.key -> pg_tls_rsa_server.pem / .key; delete ca.key
```
