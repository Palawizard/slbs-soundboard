#pragma once

#include <ntddk.h>

#define SLB_RING_MAGIC 'RBLS'
#define SLB_RING_VERSION 1
#define SLB_RING_HEADER_SIZE 128
#define SLB_RING_SAMPLE_RATE 48000
#define SLB_RING_CHANNELS 2
#define SLB_RING_SAMPLE_FORMAT_F32 1
#define SLB_RING_FLAG_PRODUCER_ACTIVE 1
#define SLB_RING_STALE_TIMEOUT_MS 2000

typedef struct DECLSPEC_ALIGN(64) _SLB_RING_HEADER
{
    ULONG Magic;
    USHORT Version;
    USHORT HeaderSize;
    ULONG SampleRate;
    USHORT Channels;
    USHORT SampleFormat;
    ULONG CapacityFrames;
    volatile LONG Flags;
    volatile LONG64 WriteFrame;
    volatile LONG64 ReadFrame;
    volatile LONG64 HeartbeatMs;
    LONG64 ProducerEpoch;
    volatile LONG64 OverrunFrames;
    volatile LONG64 UnderrunFrames;
    volatile LONG64 ConsumedFrames;
    LONG64 Reserved[6];
} SLB_RING_HEADER, *PSLB_RING_HEADER;

static_assert(sizeof(SLB_RING_HEADER) == SLB_RING_HEADER_SIZE, "SLB ring header size changed");
static_assert(FIELD_OFFSET(SLB_RING_HEADER, WriteFrame) == 24, "SLB write index offset changed");
static_assert(FIELD_OFFSET(SLB_RING_HEADER, UnderrunFrames) == 64, "SLB underrun offset changed");

typedef struct _SLB_RING_READER
{
    HANDLE SectionHandle;
    PVOID SectionObject;
    PVOID View;
    SIZE_T ViewSize;
    PMDL LockedMdl;
    PSLB_RING_HEADER Header;
    PFLOAT Samples;
} SLB_RING_READER, *PSLB_RING_READER;

VOID SlbRingInitialize(_Out_ PSLB_RING_READER Reader);
_Must_inspect_result_ NTSTATUS SlbRingOpen(_Inout_ PSLB_RING_READER Reader);
VOID SlbRingClose(_Inout_ PSLB_RING_READER Reader);
VOID SlbRingReadFloatStereo(
    _Inout_ PSLB_RING_READER Reader,
    _Out_writes_bytes_(ByteCount) PBYTE Destination,
    _In_ ULONG ByteCount
);
