# atm0s-sdn-router

This is router module for atm0s-sdn.

## Benchmark

Bellow is result of benchmarking atm0s-sdn-router with Macbook M1 Pro.

```bash
empty/next_node         time:   [4.1770 ns 4.2103 ns 4.2598 ns]
                        thrpt:  [234.75 Melem/s 237.51 Melem/s 239.40 Melem/s]
Found 5 outliers among 100 measurements (5.00%)
  2 (2.00%) high mild
  3 (3.00%) high severe
empty/next_closest      time:   [17.148 ns 17.172 ns 17.206 ns]
                        thrpt:  [58.121 Melem/s 58.236 Melem/s 58.317 Melem/s]
Found 4 outliers among 100 measurements (4.00%)
  2 (2.00%) low mild
  1 (1.00%) high mild
  1 (1.00%) high severe
empty/next_service      time:   [2.0514 ns 2.0552 ns 2.0594 ns]
                        thrpt:  [485.59 Melem/s 486.56 Melem/s 487.48 Melem/s]
Found 6 outliers among 100 measurements (6.00%)
  4 (4.00%) high mild
  2 (2.00%) high severe

single/next_node        time:   [4.8156 ns 4.8222 ns 4.8298 ns]
                        thrpt:  [207.05 Melem/s 207.37 Melem/s 207.66 Melem/s]
Found 3 outliers among 100 measurements (3.00%)
  3 (3.00%) high mild
single/next_closest     time:   [27.781 ns 27.937 ns 28.226 ns]
                        thrpt:  [35.429 Melem/s 35.794 Melem/s 35.996 Melem/s]
Found 2 outliers among 100 measurements (2.00%)
  2 (2.00%) high severe
single/next_service     time:   [2.0487 ns 2.0515 ns 2.0547 ns]
                        thrpt:  [486.68 Melem/s 487.45 Melem/s 488.12 Melem/s]
Found 3 outliers among 100 measurements (3.00%)
  3 (3.00%) high mild

full/next_node          time:   [4.8162 ns 4.8228 ns 4.8298 ns]
                        thrpt:  [207.05 Melem/s 207.35 Melem/s 207.63 Melem/s]
Found 21 outliers among 100 measurements (21.00%)
  14 (14.00%) high mild
  7 (7.00%) high severe
full/next_closest       time:   [257.14 ns 258.14 ns 259.53 ns]
                        thrpt:  [3.8532 Melem/s 3.8739 Melem/s 3.8889 Melem/s]
Found 6 outliers among 100 measurements (6.00%)
  3 (3.00%) high mild
  3 (3.00%) high severe
full/next_service       time:   [2.0579 ns 2.0616 ns 2.0651 ns]
                        thrpt:  [484.24 Melem/s 485.07 Melem/s 485.94 Melem/s]
Found 5 outliers among 100 measurements (5.00%)
  5 (5.00%) high mild
```