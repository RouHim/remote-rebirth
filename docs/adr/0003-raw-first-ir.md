# Prefer raw pulse records

Timing-bearing IR inputs (Pronto `0000`/`0100`, LIRC `raw_codes`, Flipper `raw`) become class-1 pulse records directly. Protocol encoders are used only for inputs that carry protocol fields (Pronto shorthand, LIRC `codes`, Flipper `parsed`). This preserves the captured waveform exactly and avoids approximating protocols the encoder does not cover; unsupported parsed protocols still fail explicitly.

## Consequences

Raw inputs bypass protocol validation. Class-1 records are larger than dictionary-encoded records, so the 256 KiB limit is checked after composition.
