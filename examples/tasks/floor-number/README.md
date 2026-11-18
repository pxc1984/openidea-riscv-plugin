# Задача «Этажи»

`floor_number(fpf, fps, flat)` = `((flat - 1) / fpf) % fps + 1`.

Проверка:

```sh
openidea-riscv floor-number.asm --tests tests
```

Тесты `tests/` — приложенные к задаче `NN.dat`/`NN.ans`.
