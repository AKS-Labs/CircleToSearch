// LD_PRELOAD interposer for onnxruntime 1.13.1.
// The AhoTTS `tts` binary imports a single ORT symbol, OrtGetApiBase, and routes
// every call through the OrtApi struct it returns. We interpose that one entry
// point, copy the real OrtApi, and replace only Run so we can dump the int64
// `input` tensor (the VITS token-id sequence) that cotovia feeds the model.
//
// Each Run that has an input named "input" appends one line to $ORT_DUMP_FILE
// (default stderr): "IDS <id> <id> ...".

#define _GNU_SOURCE
#include <dlfcn.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "onnxruntime_c_api.h"

static const OrtApiBase *g_real_base = NULL;
static const OrtApi *g_real = NULL;
static OrtApi g_api;

static OrtStatusPtr ORT_API_CALL my_Run(
    OrtSession *session, const OrtRunOptions *run_options,
    const char *const *input_names, const OrtValue *const *inputs, size_t input_len,
    const char *const *output_names, size_t output_names_len, OrtValue **outputs) {
  for (size_t i = 0; i < input_len; i++) {
    int is_input = strcmp(input_names[i], "input") == 0;
    int is_scales = strcmp(input_names[i], "scales") == 0;
    if (!is_input && !is_scales) {
      continue;
    }
    const OrtValue *v = inputs[i];
    OrtTensorTypeAndShapeInfo *info = NULL;
    if (g_real->GetTensorTypeAndShape(v, &info) != NULL) {
      continue;
    }
    size_t count = 0;
    g_real->GetTensorShapeElementCount(info, &count);
    void *data = NULL;
    g_real->GetTensorMutableData((OrtValue *)v, &data);

    const char *path = getenv("ORT_DUMP_FILE");
    FILE *f = path ? fopen(path, "a") : stderr;
    if (is_input) {
      const int64_t *ids = (const int64_t *)data;
      fprintf(f, "IDS");
      for (size_t k = 0; k < count; k++) {
        fprintf(f, " %lld", (long long)ids[k]);
      }
    } else {
      const float *sc = (const float *)data;
      fprintf(f, "SCALES");
      for (size_t k = 0; k < count; k++) {
        fprintf(f, " %g", sc[k]);
      }
    }
    fprintf(f, "\n");
    if (path) {
      fclose(f);
    }
    g_real->ReleaseTensorTypeAndShapeInfo(info);
  }
  return g_real->Run(session, run_options, input_names, inputs, input_len,
                     output_names, output_names_len, outputs);
}

static const OrtApi *ORT_API_CALL my_GetApi(uint32_t version) {
  const OrtApi *real = g_real_base->GetApi(version);
  if (real == NULL) {
    return NULL;
  }
  g_real = real;
  memcpy(&g_api, real, sizeof(OrtApi));
  g_api.Run = my_Run;
  return &g_api;
}

const OrtApiBase *OrtGetApiBase(void) {
  static OrtApiBase base;
  const OrtApiBase *(*real_fn)(void) =
      (const OrtApiBase *(*)(void))dlsym(RTLD_NEXT, "OrtGetApiBase");
  g_real_base = real_fn();
  base.GetApi = my_GetApi;
  base.GetVersionString = g_real_base->GetVersionString;
  return &base;
}
