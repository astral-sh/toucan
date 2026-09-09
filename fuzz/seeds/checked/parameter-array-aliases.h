typedef int Array[4]; typedef Array Chain; typedef int Row[3]; typedef Row Matrix[2];
void consume(Chain); void matrix(Matrix); typedef void Callback(Array); struct Holder { Callback *callback; };
