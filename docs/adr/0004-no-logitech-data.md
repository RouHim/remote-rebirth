# Never ship Logitech catalogue data

The CLI ships no device catalogue, IR database, or Logitech-derived rhythm table; all IR data comes from user-supplied open-format files, and the six protocol encoders are written from public timing facts. The reference project's generated rhythm table is derived from Logitech's catalogue and is excluded even though its repository is MIT.

## Consequences

Coverage is limited to the six encoders plus raw timings; users must supply their own IR files. GPLv3 compatibility is preserved.
