// rsf-audio: records one PipeWire node's output (the game's stream) to a float WAV file, with the wall-clock time of
// every buffer, for scripts/rsf-session.sh. A stream that pauses (stage loads) leaves no samples in the WAV and the
// sound card's clock drifts from the system's, so the times file is what aligns the audio with the session's CLK1:
// little-endian records of u64 CLOCK_REALTIME ns when the buffer arrived, u64 its first frame in the WAV, u32 frames.
// Usage: rsf-audio TARGET_SERIAL RATE CHANNELS OUT.wav OUT.times (stops on SIGINT/SIGTERM or when the node goes).
// Build (test machine): cc -O2 -o rsf-audio rsf-audio.c $(pkg-config --cflags --libs libpipewire-0.3)
#include <pipewire/pipewire.h>
#include <spa/param/audio/format-utils.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <time.h>

struct capture
{
    struct pw_main_loop* loop;
    struct pw_stream* stream;
    FILE *wav, *times;
    uint32_t rate, channels;
    uint64_t frames;
};

static void wav_header(struct capture* c)
{
    uint32_t data = (uint32_t)(c->frames * 4 * c->channels), block = 4 * c->channels;
    uint32_t h[11] = { 0x46464952 /* RIFF */, 36 + data, 0x45564157 /* WAVE */, 0x20746d66 /* fmt  */, 16,
        3 /* IEEE float */ | (c->channels << 16), c->rate, c->rate * block, block | (32 << 16), 0x61746164 /* data */,
        data };
    fseek(c->wav, 0, SEEK_SET);
    fwrite(h, 4, 11, c->wav);
    fseek(c->wav, 0, SEEK_END);
}

static void on_process(void* userdata)
{
    struct capture* c = userdata;
    struct pw_buffer* b = pw_stream_dequeue_buffer(c->stream);
    if (!b)
        return;
    struct spa_data* d = &b->buffer->datas[0];
    uint32_t n = d->data && d->chunk ? d->chunk->size / (4 * c->channels) : 0;
    if (n)
    {
        struct timespec ts;
        clock_gettime(CLOCK_REALTIME, &ts);
        uint64_t rec[2] = { (uint64_t)ts.tv_sec * 1000000000u + (uint64_t)ts.tv_nsec, c->frames };
        fwrite((char*)d->data + d->chunk->offset, 4 * c->channels, n, c->wav);
        fwrite(rec, 8, 2, c->times);
        fwrite(&n, 4, 1, c->times);
        c->frames += n;
    }
    pw_stream_queue_buffer(c->stream, b);
}

static void on_state(void* userdata, enum pw_stream_state old, enum pw_stream_state state, const char* error)
{
    struct capture* c = userdata;
    (void)old;
    if (state == PW_STREAM_STATE_ERROR || state == PW_STREAM_STATE_UNCONNECTED)
    {
        fprintf(stderr, "rsf-audio: stream %s%s%s\n", pw_stream_state_as_string(state), error ? ": " : "",
            error ? error : "");
        pw_main_loop_quit(c->loop);
    }
}

static const struct pw_stream_events events = {
    PW_VERSION_STREAM_EVENTS,
    .state_changed = on_state,
    .process = on_process,
};

static void on_signal(void* userdata, int signal)
{
    (void)signal;
    pw_main_loop_quit(((struct capture*)userdata)->loop);
}

int main(int argc, char** argv)
{
    if (argc != 6)
    {
        fprintf(stderr, "usage: rsf-audio TARGET_SERIAL RATE CHANNELS OUT.wav OUT.times\n");
        return 2;
    }
    struct capture c = { 0 };
    c.rate = (uint32_t)atoi(argv[2]);
    c.channels = (uint32_t)atoi(argv[3]);
    c.wav = fopen(argv[4], "wb");
    c.times = fopen(argv[5], "wb");
    if (!c.wav || !c.times || !c.rate || !c.channels)
        return 1;
    wav_header(&c);
    pw_init(&argc, &argv);
    c.loop = pw_main_loop_new(NULL);
    pw_loop_add_signal(pw_main_loop_get_loop(c.loop), SIGINT, on_signal, &c);
    pw_loop_add_signal(pw_main_loop_get_loop(c.loop), SIGTERM, on_signal, &c);
    struct pw_properties* props = pw_properties_new(PW_KEY_MEDIA_TYPE, "Audio", PW_KEY_MEDIA_CATEGORY, "Capture",
        PW_KEY_MEDIA_ROLE, "Music", PW_KEY_TARGET_OBJECT, argv[1], PW_KEY_NODE_DONT_RECONNECT, "true",
        PW_KEY_NODE_NAME, "rsf-audio", NULL);
    c.stream = pw_stream_new_simple(pw_main_loop_get_loop(c.loop), "rsf-audio", props, &events, &c);
    uint8_t buffer[1024];
    struct spa_pod_builder builder = SPA_POD_BUILDER_INIT(buffer, sizeof(buffer));
    struct spa_audio_info_raw info = { .format = SPA_AUDIO_FORMAT_F32, .rate = c.rate, .channels = c.channels };
    if (c.channels == 2)  // positioned, so nothing is remixed
    {
        info.position[0] = SPA_AUDIO_CHANNEL_FL;
        info.position[1] = SPA_AUDIO_CHANNEL_FR;
    }
    const struct spa_pod* params[1] = { spa_format_audio_raw_build(&builder, SPA_PARAM_EnumFormat, &info) };
    pw_stream_connect(c.stream, PW_DIRECTION_INPUT, PW_ID_ANY,
        PW_STREAM_FLAG_AUTOCONNECT | PW_STREAM_FLAG_MAP_BUFFERS, params, 1);
    pw_main_loop_run(c.loop);
    pw_stream_destroy(c.stream);
    pw_main_loop_destroy(c.loop);
    wav_header(&c);
    fclose(c.wav);
    fclose(c.times);
    pw_deinit();
    fprintf(stderr, "rsf-audio: %llu frames\n", (unsigned long long)c.frames);
    return 0;
}
