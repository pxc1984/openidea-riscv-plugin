# int is_valid_triangle(int a, int b, int c)
# Только базовые инструкции, без ветвлений: каждый признак (a+b>c, b+c>a, a+c>b)
# кодируется битом s = ((-d) >> 63) & 1, где d - соответствующая разность; s = 1 при d > 0.
# Результат - AND всех трёх знаковых битовых масок: 0 или -1 (любое ненулевое годится).
        .globl is_valid_triangle
is_valid_triangle:
        sub t0, a1, a2
        add t0, a0, t0          # a + b - c
        sub t1, a2, a0
        add t1, a1, t1          # b + c - a
        sub t2, a0, a1
        add t2, a2, t2          # a + c - b
        neg t0, t0
        srai t0, t0, 63
        neg t1, t1
        srai t1, t1, 63
        neg t2, t2
        srai t2, t2, 63
        and a0, t0, t1
        and a0, a0, t2
        ret
