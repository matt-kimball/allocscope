#include <stdlib.h>

int main() {
    for (int i = 0; i < 1024; i++) {
        void *mem = malloc(1024 * 1024);
        free(mem);
    }

    return 0;
}
