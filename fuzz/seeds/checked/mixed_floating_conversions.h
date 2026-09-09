enum Choice { first, second };
struct Bits { unsigned value:3; };
void sink(float, double, ...);
long double arithmetic(short small, unsigned char byte, enum Choice choice,
                       struct Bits bits, float single, double wide, long double extended) {
    float sum = small + single;
    double difference = byte - wide;
    long double product = choice * extended;
    double conditional = bits.value ? byte : wide;
    small += single;
    byte = (unsigned char)wide;
    sink(small, byte, single, choice, bits.value);
    return sum + difference + product + conditional + (small < single) + +byte;
}
