typedef int Array[4];
void original(Array);
int first = sizeof(void (*)(Array)), second[sizeof(void (*)(Array))];
__typeof__(sizeof(__typeof__(original) *)) count;
struct Holder {
    void (*callback)(Array);
    int values[sizeof(void (*)(Array))];
};
__typeof__(original) after;
