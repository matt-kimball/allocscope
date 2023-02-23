#include <stdlib.h>

void iter(size_t i) {
    char *mem = new char[1024 * 1024];
    delete[] mem;
}


int main() {
    for (int i = 0; i < 1024; i++) {
        iter(i);
    }

    return 0;
}
