# Write the whole container and read the chunk acknowledgement at offset 2

A read of `/cfg/usercfg` is four bytes short because the open size is `end_addr - 0x020000`; the write
must send the whole container including `PTYY`, and the Read-Back appends the marker before comparison.
Only the final chunk carries the done packet: the write request's packet count includes the command and,
on the final chunk, the done packet. The chunk acknowledgement is `FF 03 <last sequence | 0x80>` and
`FF 03 FE` on the final chunk, so the expected byte is at offset 2, not offset 0.

## Evidence

On 2026-11-05 the first committed write failed at the chunk acknowledgement because the expected byte was
read at offset 0; the open had already erased the Configuration. `restore` of the 24725-byte Backup
recovered the unit and was verified by Read-Back. After this correction an 803-byte write completed and
an independent Backup of the written Configuration was byte-identical; restoring the 24725-byte original
was byte-identical again.
