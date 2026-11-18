# struct Elem { unsigned count; unsigned *data; unsigned summ; };
# void process(struct Elem *elems, size_t size)
# Поле summ (offset 16) получает младшие 32 бита суммы массива data.
        .globl process
process:
        beqz a1, done
elem:
        ld t0, 0(a0)            # count
        ld t1, 8(a0)            # data
        li t2, 0
        li t3, 0
sum:
        bgeu t3, t0, store
        slli t4, t3, 2
        add t4, t1, t4
        lw t4, 0(t4)
        add t2, t2, t4
        addi t3, t3, 1
        j sum
store:
        sw t2, 16(a0)
        addi a0, a0, 24
        addi a1, a1, -1
        bnez a1, elem
done:
        ret

# Пример из условия: печатает 10 и 35.
        .data
        .align 3
first:  .word 1, 2, 3, 4
second: .word 5, 6, 7, 8, 9
        .align 3
elems:  .dword 4, first, 0
        .dword 5, second, 0

        .text
main:
        la a0, elems
        li a1, 2
        call process
        la t0, elems
        ld a0, 16(t0)           # summ первого: 10
        li a7, 1
        ecall
        li a0, 10
        li a7, 11
        ecall
        ld a0, 40(t0)           # summ второго: 35
        li a7, 1
        ecall
        li a0, 10
        li a7, 11
        ecall
        li a0, 0
        li a7, 10
        ecall
