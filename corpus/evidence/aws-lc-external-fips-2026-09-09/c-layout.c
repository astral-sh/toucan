#include <stdio.h>
#include "openssl/sha.h"
#include "openssl/aead.h"
#include "openssl/bytestring.h"
#define LAYOUT(T) printf("layout %s %zu %zu\n", #T, sizeof(T), _Alignof(T))
int main(void) {
  LAYOUT(SHA_CTX);
  LAYOUT(SHA256_CTX);
  LAYOUT(SHA512_CTX);
  LAYOUT(EVP_AEAD_CTX);
  LAYOUT(CBS);
  LAYOUT(CBB);
  return 0;
}
