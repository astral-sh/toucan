#include <stddef.h>
#include <stdio.h>
#include "rust_wrapper.h"

#define LAYOUT(type) printf("layout " #type " %zu %zu\n", sizeof(type), _Alignof(type))

int main(void) {
    LAYOUT(SHA_CTX);
    LAYOUT(SHA256_CTX);
    LAYOUT(SHA512_CTX);
    LAYOUT(EVP_AEAD_CTX);
    LAYOUT(CBS);
    LAYOUT(CBB);
    return 0;
}
