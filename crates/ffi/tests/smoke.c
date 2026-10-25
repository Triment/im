/* Build: gcc smoke.c -I.. -L../../../target/debug -lxmp_ffi -Wl,-rpath,../../../target/debug -lm -o smoke
 * Usage: smoke <cert.der> <caller> <password> <callee_username> */
#include "xmp.h"
#include <math.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <stdatomic.h>

static atomic_int audio_frames, video_frames, events;
static atomic_int vw, vh;

static void on_event(void *ud, const char *json) { (void)ud; atomic_fetch_add(&events, 1); printf("event: %s\n", json); }
static void on_audio(void *ud, const int16_t *pcm, size_t n) { (void)ud; (void)pcm; (void)n; atomic_fetch_add(&audio_frames, 1); }
static void on_video(void *ud, const uint8_t *y, const uint8_t *u, const uint8_t *v, int32_t w, int32_t h) {
    (void)ud; (void)y; (void)u; (void)v; atomic_fetch_add(&video_frames, 1); vw = w; vh = h;
}
static void die(const char *what) { fprintf(stderr, "FAIL %s: %s\n", what, xmp_last_error()); exit(1); }

int main(int argc, char **argv) {
    if (argc < 5) return 2;
    FILE *f = fopen(argv[1], "rb"); uint8_t cert[4096]; size_t n = fread(cert, 1, sizeof cert, f); fclose(f);
    XmpClient *c = xmp_connect("127.0.0.1:4433", "localhost", cert, n, 0, argv[2], argv[3], on_event, NULL);
    if (!c) die("connect");
    printf("logged in, user_id=%lld\n", (long long)xmp_user_id(c));
    char *fr = xmp_friends_json(c); printf("friends: %s\n", fr);
    /* find callee id: crude scan for "id":N before the username */
    char needle[128]; snprintf(needle, sizeof needle, "\"username\":\"%s\"", argv[4]);
    char *p = strstr(fr, needle); if (!p) die("callee not a friend");
    while (p > fr && strncmp(p, "\"id\":", 5) != 0) p--;
    long long peer = atoll(p + 5); xmp_string_free(fr);
    long long seq = xmp_send_text(c, peer, "hello from the C API \xE2\x9C\x93"); if (seq < 0) die("send_text");
    printf("sent text, seq=%lld\n", seq);

    XmpCall *call = xmp_call_start(c, peer, 1, on_audio, on_video, NULL);
    if (!call) die("call_start");
    char *sas = xmp_call_sas(call); printf("call up, SAS=%s\n", sas); xmp_string_free(sas);

    const int W = 640, H = 360;
    uint8_t *Y = malloc(W * H), *U = malloc(W * H / 4), *V = malloc(W * H / 4);
    memset(U, 128, W * H / 4); memset(V, 128, W * H / 4);
    int16_t pcm[1920];
    for (int tick = 0; tick < 150; tick++) {            /* 3 s: 20 ms audio ticks, video every 33 ms */
        for (int i = 0; i < 960; i++) {
            double t = (tick * 960 + i) / 48000.0;
            pcm[2 * i] = (int16_t)(sin(t * 6.2831853 * 440) * 10000);
            pcm[2 * i + 1] = (int16_t)(sin(t * 6.2831853 * 660) * 10000);
        }
        if (xmp_call_push_audio(call, pcm, 1920) != 0) die("push_audio");
        if (tick % 2 == 0 || tick % 3 == 0) {
            for (int y = 0; y < H; y++) for (int x = 0; x < W; x++) Y[y * W + x] = (uint8_t)((x + y + tick * 3) * 255 / (W + H));
            if (xmp_call_push_video(call, Y, U, V, W, H, tick == 0) != 0) die("push_video");
        }
        usleep(20000);
    }
    sleep(1);
    char *st = xmp_call_stats_json(call); printf("stats: %s\n", st); xmp_string_free(st);
    printf("RESULT audio_frames=%d video_frames=%d (%dx%d) events=%d\n", audio_frames, video_frames, vw, vh, events);
    xmp_call_hangup(call);
    xmp_free(c);
    return (audio_frames > 100 && video_frames > 50) ? 0 : 1;
}
