        .text
        .globl fact

# uint64_t fact(uint64_t n)
fact:   li      t0, 1           # результат
        li      t1, 1           # i
loop:   bgtu    t1, a0, done    # i > n ? -> конец
        mul     t0, t0, t1      # res *= i
        addi    t1, t1, 1
        j       loop
done:   mv      a0, t0
        ret
