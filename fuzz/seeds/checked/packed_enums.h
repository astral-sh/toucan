enum __attribute__((packed)) Byte { LOW=(_Bool)0, HIGH=255 };
enum __attribute__((packed)) SignedWord { MINIMUM=-32768, MAXIMUM=32767 };
typedef enum Byte Byte;
struct Fields { Byte bytes[3]; enum SignedWord word; unsigned flags:3; };
int variadic(int,...);
int use(Byte value,struct Fields *fields) {
    return variadic(0,value,fields->word)+(value+1);
}
