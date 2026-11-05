# Reconstruct the PTYY trailer

`/cfg/usercfg` reports `end_addr − 0x020000`, so a read omits the four-byte `PTYY` marker and is not a valid container. `backup` appends the marker before writing the file and hashing it; `write` appends it to the read-back before comparing byte-for-byte. Comparing only the first `len−4` bytes would let a missing or corrupt marker pass verification.

## Consequences

Sidecar size and digest cover the reconstructed container. A read that is not a recognisable container is stored raw and marked unreadable.
