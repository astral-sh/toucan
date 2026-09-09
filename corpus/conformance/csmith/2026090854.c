/*
 * This is a RANDOMLY GENERATED PROGRAM.
 *
 * Generator: csmith 2.3.0
 * Git version: 30dccd7
 * Options:   --seed 2026090854 --no-packed-struct --match-exact-qualifiers --strict-volatile-rule --max-funcs 5 --max-block-depth 3 --max-block-size 3 --max-expr-complexity 6 --max-array-dim 3 --max-array-len-per-dim 4 --output /home/dev-user/.cache/toucan/csmith-audit-dbda83c/run100/2026090854/source.c
 * Seed:      2026090854
 */

#include "csmith.h"


static long __undefined;

/* --- Struct/Union Declarations --- */
struct S0 {
   const unsigned f0 : 1;
   const volatile int64_t  f1;
   int64_t  f2;
   int32_t  f3;
   uint64_t  f4;
   uint32_t  f5;
   const uint32_t  f6;
   const int64_t  f7;
   volatile int32_t  f8;
   uint8_t  f9;
};

struct S6 {
   volatile int32_t  f0;
   const volatile struct S0  f1;
   uint32_t  f2;
};

/* --- GLOBAL VARIABLES --- */
static int32_t g_3[2][3][4] = {{{0x12B94821L,4L,0x14BA09A1L,0x14BA09A1L},{0x2B500989L,0x2B500989L,0x12B94821L,0x14BA09A1L},{0xC3C3AE50L,4L,0xC3C3AE50L,0x12B94821L}},{{0xC3C3AE50L,0x12B94821L,0x12B94821L,0xC3C3AE50L},{0x2B500989L,0x12B94821L,0x14BA09A1L,0x12B94821L},{0x12B94821L,4L,0x14BA09A1L,0x14BA09A1L}}};
static int32_t *g_5 = &g_3[0][0][1];
static struct S6 g_6[4] = {{0x6A426232L,{0,0xA4E630971CF717C9LL,0x5701832D748AEE51LL,0x904E20EEL,0x4983CA307758A8CELL,18446744073709551615UL,18446744073709551615UL,0x227F44FB66C146DFLL,1L,0x85L},0x69896260L},{0x6A426232L,{0,0xA4E630971CF717C9LL,0x5701832D748AEE51LL,0x904E20EEL,0x4983CA307758A8CELL,18446744073709551615UL,18446744073709551615UL,0x227F44FB66C146DFLL,1L,0x85L},0x69896260L},{0x6A426232L,{0,0xA4E630971CF717C9LL,0x5701832D748AEE51LL,0x904E20EEL,0x4983CA307758A8CELL,18446744073709551615UL,18446744073709551615UL,0x227F44FB66C146DFLL,1L,0x85L},0x69896260L},{0x6A426232L,{0,0xA4E630971CF717C9LL,0x5701832D748AEE51LL,0x904E20EEL,0x4983CA307758A8CELL,18446744073709551615UL,18446744073709551615UL,0x227F44FB66C146DFLL,1L,0x85L},0x69896260L}};


/* --- FORWARD DECLARATIONS --- */
static struct S6  func_1(void);


/* --- FUNCTIONS --- */
/* ------------------------------------------ */
/* 
 * reads : g_6
 * writes: g_5
 */
static struct S6  func_1(void)
{ /* block id: 0 */
    int32_t *l_2[1][4] = {{&g_3[0][0][1],&g_3[0][0][1],&g_3[0][0][1],&g_3[0][0][1]}};
    int32_t **l_4[2];
    int i, j;
    for (i = 0; i < 2; i++)
        l_4[i] = (void*)0;
    g_5 = l_2[0][1];
    return g_6[1];
}




/* ---------------------------------------- */
int main (int argc, char* argv[])
{
    int i, j, k;
    int print_hash_value = 0;
    if (argc == 2 && strcmp(argv[1], "1") == 0) print_hash_value = 1;
    platform_main_begin();
    crc32_gentab();
    func_1();
    for (i = 0; i < 2; i++)
    {
        for (j = 0; j < 3; j++)
        {
            for (k = 0; k < 4; k++)
            {
                transparent_crc(g_3[i][j][k], "g_3[i][j][k]", print_hash_value);
                if (print_hash_value) printf("index = [%d][%d][%d]\n", i, j, k);

            }
        }
    }
    for (i = 0; i < 4; i++)
    {
        transparent_crc(g_6[i].f0, "g_6[i].f0", print_hash_value);
        transparent_crc(g_6[i].f1.f0, "g_6[i].f1.f0", print_hash_value);
        transparent_crc(g_6[i].f1.f1, "g_6[i].f1.f1", print_hash_value);
        transparent_crc(g_6[i].f1.f2, "g_6[i].f1.f2", print_hash_value);
        transparent_crc(g_6[i].f1.f3, "g_6[i].f1.f3", print_hash_value);
        transparent_crc(g_6[i].f1.f4, "g_6[i].f1.f4", print_hash_value);
        transparent_crc(g_6[i].f1.f5, "g_6[i].f1.f5", print_hash_value);
        transparent_crc(g_6[i].f1.f6, "g_6[i].f1.f6", print_hash_value);
        transparent_crc(g_6[i].f1.f7, "g_6[i].f1.f7", print_hash_value);
        transparent_crc(g_6[i].f1.f8, "g_6[i].f1.f8", print_hash_value);
        transparent_crc(g_6[i].f1.f9, "g_6[i].f1.f9", print_hash_value);
        transparent_crc(g_6[i].f2, "g_6[i].f2", print_hash_value);
        if (print_hash_value) printf("index = [%d]\n", i);

    }
    platform_main_end(crc32_context ^ 0xFFFFFFFFUL, print_hash_value);
    return 0;
}

/************************ statistics *************************
XXX max struct depth: 2
breakdown:
   depth: 0, occurrence: 2
   depth: 1, occurrence: 0
   depth: 2, occurrence: 1
XXX total union variables: 0

XXX non-zero bitfields defined in structs: 0
XXX zero bitfields defined in structs: 0
XXX const bitfields defined in structs: 0
XXX volatile bitfields defined in structs: 0
XXX structs with bitfields in the program: 1
breakdown:
   indirect level: 0, occurrence: 1
XXX full-bitfields structs in the program: 0
breakdown:
XXX times a bitfields struct's address is taken: 0
XXX times a bitfields struct on LHS: 0
XXX times a bitfields struct on RHS: 1
XXX times a single bitfield on LHS: 0
XXX times a single bitfield on RHS: 0

XXX max expression depth: 1
breakdown:
   depth: 1, occurrence: 3

XXX total number of pointers: 3

XXX times a variable address is taken: 3
XXX times a pointer is dereferenced on RHS: 0
breakdown:
XXX times a pointer is dereferenced on LHS: 0
breakdown:
XXX times a pointer is compared with null: 0
XXX times a pointer is compared with address of another variable: 0
XXX times a pointer is compared with another pointer: 0
XXX times a pointer is qualified to be dereferenced: 0
XXX number of pointers point to pointers: 1
XXX number of pointers point to scalars: 2
XXX number of pointers point to structs: 0
XXX percent of pointers has null in alias set: 33.3
XXX average alias set size: 1

XXX times a non-volatile is read: 2
XXX times a non-volatile is write: 1
XXX times a volatile is read: 0
XXX    times read thru a pointer: 0
XXX times a volatile is write: 0
XXX    times written thru a pointer: 0
XXX times a volatile is available for access: 0
XXX percentage of non-volatile access: 100

XXX forward jumps: 0
XXX backward jumps: 0

XXX stmts: 2
XXX max block depth: 0
breakdown:
   depth: 0, occurrence: 2

XXX percentage a fresh-made variable is used: 100
XXX percentage an existing variable is used: 0
FYI: the random generator makes assumptions about the integer size. See platform.info for more details.
********************* end of statistics **********************/

