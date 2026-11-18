# unsigned floor_number(unsigned flats_per_floor, unsigned floors_per_section, unsigned flat_number)
        .globl floor_number
floor_number:
        addi a2, a2, -1
        divu a2, a2, a0
        remu a2, a2, a1
        addi a0, a2, 1
        ret

# main для прогона тестов: читает тройки до EOF (syscall 5 возвращает -1).
main:
loop:
        li a7, 5
        ecall
        mv t0, a0
        li a7, 5
        ecall
        mv t2, a0
        li a7, 5
        ecall
        li t1, -1
        beq t0, t1, done
        mv a2, a0
        mv a0, t0
        mv a1, t2
        call floor_number
        li a7, 1
        ecall
        li a0, 10
        li a7, 11
        ecall
        j loop
done:
        li a0, 0
        li a7, 10
        ecall
