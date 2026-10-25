/* XMP client C API. All functions are thread-safe; failures return NULL / -1 and set xmp_last_error(). */
#ifndef XMP_H
#define XMP_H
#include <stddef.h>
#include <stdint.h>
#ifdef __cplusplus
extern "C" {
#endif

typedef struct XmpClient XmpClient;
typedef struct XmpCall XmpCall;

/* Server pushes as JSON: message / presence / incoming_call / call_ended / room_event. */
typedef void (*xmp_event_cb)(void *ud, const char *json);
/* Decoded 48 kHz stereo interleaved i16 PCM, one 20 ms frame (1920 samples). */
typedef void (*xmp_audio_cb)(void *ud, const int16_t *pcm, size_t samples);
/* Decoded I420 frame (planes tightly packed; pointers valid only during the callback). */
typedef void (*xmp_video_cb)(void *ud, const uint8_t *y, const uint8_t *u, const uint8_t *v, int32_t w, int32_t h);

const char *xmp_last_error(void);
void xmp_string_free(char *s);

/* mode: 0 login, 1 register. cert: pinned DER certificate or NULL for public CAs. */
XmpClient *xmp_connect(const char *server, const char *server_name, const uint8_t *cert, size_t cert_len,
                       int32_t mode, const char *user, const char *password, xmp_event_cb cb, void *ud);
void xmp_free(XmpClient *c);
int64_t xmp_user_id(const XmpClient *c);
char *xmp_token(const XmpClient *c);

int32_t xmp_friend_request(const XmpClient *c, const char *username);
int32_t xmp_friend_accept(const XmpClient *c, int64_t user_id);
char *xmp_friends_json(const XmpClient *c);
int64_t xmp_send_text(const XmpClient *c, int64_t to_user, const char *text); /* returns seq */
int32_t xmp_send_file(const XmpClient *c, int64_t to_user, const char *path);
int32_t xmp_download_file(const XmpClient *c, const char *hash_hex, uint64_t size, const char *dest);
char *xmp_sync_json(const XmpClient *c, int64_t conv_id, int64_t after_seq, uint32_t limit);
char *xmp_conversations_json(const XmpClient *c);

/* Blocking: call from a worker thread. */
XmpCall *xmp_call_start(const XmpClient *c, int64_t peer, int32_t video, xmp_audio_cb acb, xmp_video_cb vcb, void *ud);
XmpCall *xmp_call_accept(const XmpClient *c, uint64_t call_id, int64_t peer, int32_t video, xmp_audio_cb acb, xmp_video_cb vcb, void *ud);
int32_t xmp_call_reject(const XmpClient *c, uint64_t call_id, int64_t peer);
char *xmp_call_sas(const XmpCall *call);
int32_t xmp_call_push_audio(const XmpCall *call, const int16_t *pcm, size_t samples); /* 1920 samples */
int32_t xmp_call_push_video(const XmpCall *call, const uint8_t *y, const uint8_t *u, const uint8_t *v,
                            int32_t w, int32_t h, int32_t force_key);
char *xmp_call_stats_json(const XmpCall *call);
void xmp_call_hangup(XmpCall *call);

#ifdef __cplusplus
}
#endif
#endif
