#include <stdint.h>
#include <stddef.h>

extern uint8_t SIGNALS[4096];

void hit_block(size_t block_id) {
    SIGNALS[block_id]++;
}

static void decoy_vowel(const uint8_t *data, size_t size) {
    hit_block(300);
    if (size < 20) {
        hit_block(301);
        return;
    }
    hit_block(302);
    if (data[2] % 2 == 0) {
        hit_block(303);
    } else {
        hit_block(304);
    }
    if (data[3] % 3 == 0) {
        hit_block(305);
    } else {
        hit_block(306);
    }
    if (data[4] % 5 == 0) {
        hit_block(307);
    } else {
        hit_block(308);
    }
    hit_block(309);
}

static void decoy_digit(const uint8_t *data, size_t size) {
    hit_block(320);
    if (data[1] % 4 == 0) {
        hit_block(321);
    } else {
        hit_block(322);
    }
    if (size < 25) {
        hit_block(323);
        return;
    }
    hit_block(324);
    if (data[5] % 2 == 0) {
        hit_block(325);
    } else {
        hit_block(326);
    }
    if (data[6] % 7 == 0) {
        hit_block(327);
    }
    hit_block(328);
}

void target_function(const uint8_t *data, size_t size) {
    hit_block(100);

    if (size < 10) return;
    hit_block(101);

    if (data[0] >= 'a' && data[0] <= 'z') {
        decoy_vowel(data, size);
        return;
    }
    if (data[0] >= '0' && data[0] <= '9') {
        decoy_digit(data, size);
        return;
    }
    hit_block(110);

    if (data[0] == 'T') {
        hit_block(102);

        if (data[1] == 'A') {
            hit_block(103);

            if (data[2] == 'L') {
                hit_block(104);

                if (data[3] == 'O') {
                    hit_block(105);

                    if (data[4] == 'N') {
                        hit_block(106);

                        if (data[5] == 0x01) {
                            hit_block(107);

                            if (data[6] == 0xDE) {
                                hit_block(108);

                                if (data[7] == 0xAD) {
                                    hit_block(109);

                                    if (data[8] == 0xBE) {
                                        hit_block(120);

                                        if (data[9] == 0xEF) {
                                            hit_block(200);
                                            volatile int *crash_ptr = (int*)0;
                                            *crash_ptr = 42;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
