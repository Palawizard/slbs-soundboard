#include <sysvad.h>
#include "slb_ring.h"

extern "C" NTKERNELAPI NTSTATUS MmMapViewInSystemSpace(
    _In_ PVOID Section,
    _Outptr_result_bytebuffer_(*ViewSize) PVOID* MappedBase,
    _Inout_ PSIZE_T ViewSize);
extern "C" NTKERNELAPI NTSTATUS MmUnmapViewInSystemSpace(_In_ PVOID MappedBase);

namespace
{
    constexpr WCHAR RingSectionName[] = L"\\BaseNamedObjects\\SLBVirtualAudioRingV1";

    BOOLEAN IsHeaderValid(_In_ PSLB_RING_HEADER Header, _In_ SIZE_T ViewSize)
    {
        if (Header->Magic != SLB_RING_MAGIC ||
            Header->Version != SLB_RING_VERSION ||
            Header->HeaderSize != SLB_RING_HEADER_SIZE ||
            Header->SampleRate != SLB_RING_SAMPLE_RATE ||
            Header->Channels != SLB_RING_CHANNELS ||
            Header->SampleFormat != SLB_RING_SAMPLE_FORMAT_F32 ||
            Header->CapacityFrames == 0 ||
            (Header->CapacityFrames & (Header->CapacityFrames - 1)) != 0)
        {
            return FALSE;
        }

        SIZE_T sampleBytes = 0;
        SIZE_T requiredBytes = 0;
        if (!NT_SUCCESS(RtlSizeTMult(Header->CapacityFrames, SLB_RING_CHANNELS * sizeof(FLOAT), &sampleBytes)) ||
            !NT_SUCCESS(RtlSizeTAdd(SLB_RING_HEADER_SIZE, sampleBytes, &requiredBytes)))
        {
            return FALSE;
        }
        return requiredBytes <= ViewSize;
    }
}

#pragma code_seg("PAGE")
VOID SlbRingInitialize(_Out_ PSLB_RING_READER Reader)
{
    PAGED_CODE();
    RtlZeroMemory(Reader, sizeof(*Reader));
}

#pragma code_seg("PAGE")
NTSTATUS SlbRingOpen(_Inout_ PSLB_RING_READER Reader)
{
    PAGED_CODE();
    SlbRingClose(Reader);

    UNICODE_STRING sectionName;
    RtlInitUnicodeString(&sectionName, RingSectionName);
    OBJECT_ATTRIBUTES attributes;
    InitializeObjectAttributes(
        &attributes,
        &sectionName,
        OBJ_CASE_INSENSITIVE | OBJ_KERNEL_HANDLE,
        nullptr,
        nullptr);

    NTSTATUS status = ZwOpenSection(
        &Reader->SectionHandle,
        SECTION_MAP_READ | SECTION_MAP_WRITE,
        &attributes);
    if (!NT_SUCCESS(status))
    {
        return status;
    }

    status = ObReferenceObjectByHandle(
        Reader->SectionHandle,
        SECTION_MAP_READ | SECTION_MAP_WRITE,
        nullptr,
        KernelMode,
        &Reader->SectionObject,
        nullptr);
    if (!NT_SUCCESS(status))
    {
        SlbRingClose(Reader);
        return status;
    }

    Reader->ViewSize = 0;
    status = MmMapViewInSystemSpace(Reader->SectionObject, &Reader->View, &Reader->ViewSize);
    if (!NT_SUCCESS(status))
    {
        SlbRingClose(Reader);
        return status;
    }

    Reader->Header = static_cast<PSLB_RING_HEADER>(Reader->View);
    if (!IsHeaderValid(Reader->Header, Reader->ViewSize))
    {
        SlbRingClose(Reader);
        return STATUS_REVISION_MISMATCH;
    }

    SIZE_T lockedBytes = SLB_RING_HEADER_SIZE +
        static_cast<SIZE_T>(Reader->Header->CapacityFrames) * SLB_RING_CHANNELS * sizeof(FLOAT);
    Reader->LockedMdl = IoAllocateMdl(Reader->View, static_cast<ULONG>(lockedBytes), FALSE, FALSE, nullptr);
    if (Reader->LockedMdl == nullptr)
    {
        SlbRingClose(Reader);
        return STATUS_INSUFFICIENT_RESOURCES;
    }

    __try
    {
        MmProbeAndLockPages(Reader->LockedMdl, KernelMode, IoModifyAccess);
    }
    __except (EXCEPTION_EXECUTE_HANDLER)
    {
        status = GetExceptionCode();
        IoFreeMdl(Reader->LockedMdl);
        Reader->LockedMdl = nullptr;
        SlbRingClose(Reader);
        return status;
    }

    Reader->Samples = reinterpret_cast<PFLOAT>(
        static_cast<PBYTE>(Reader->View) + SLB_RING_HEADER_SIZE);
    return STATUS_SUCCESS;
}

#pragma code_seg("PAGE")
VOID SlbRingClose(_Inout_ PSLB_RING_READER Reader)
{
    PAGED_CODE();
    Reader->Samples = nullptr;
    Reader->Header = nullptr;
    if (Reader->LockedMdl != nullptr)
    {
        MmUnlockPages(Reader->LockedMdl);
        IoFreeMdl(Reader->LockedMdl);
        Reader->LockedMdl = nullptr;
    }
    if (Reader->View != nullptr)
    {
        MmUnmapViewInSystemSpace(Reader->View);
        Reader->View = nullptr;
        Reader->ViewSize = 0;
    }
    if (Reader->SectionObject != nullptr)
    {
        ObDereferenceObject(Reader->SectionObject);
        Reader->SectionObject = nullptr;
    }
    if (Reader->SectionHandle != nullptr)
    {
        ZwClose(Reader->SectionHandle);
        Reader->SectionHandle = nullptr;
    }
}

#pragma code_seg()
VOID SlbRingReadFloatStereo(
    _Inout_ PSLB_RING_READER Reader,
    _Out_writes_bytes_(ByteCount) PBYTE Destination,
    _In_ ULONG ByteCount)
{
    constexpr ULONG frameBytes = SLB_RING_CHANNELS * sizeof(FLOAT);
    RtlZeroMemory(Destination, ByteCount);
    if (Reader->Header == nullptr || Reader->Samples == nullptr || ByteCount % frameBytes != 0)
    {
        return;
    }

    PSLB_RING_HEADER header = Reader->Header;
    ULONG requestedFrames = ByteCount / frameBytes;
    LONG flags = InterlockedCompareExchange(&header->Flags, 0, 0);
    LONG64 heartbeat = InterlockedCompareExchange64(&header->HeartbeatMs, 0, 0);
    ULONGLONG nowMs = KeQueryInterruptTime() / 10000;
    if ((flags & SLB_RING_FLAG_PRODUCER_ACTIVE) == 0 ||
        heartbeat <= 0 ||
        nowMs > static_cast<ULONGLONG>(heartbeat) + SLB_RING_STALE_TIMEOUT_MS)
    {
        InterlockedExchangeAdd64(&header->UnderrunFrames, requestedFrames);
        return;
    }

    LONG64 readFrame = InterlockedCompareExchange64(&header->ReadFrame, 0, 0);
    LONG64 writeFrame = InterlockedCompareExchange64(&header->WriteFrame, 0, 0);
    KeMemoryBarrier();
    if (writeFrame < readFrame ||
        static_cast<ULONGLONG>(writeFrame - readFrame) > header->CapacityFrames)
    {
        InterlockedExchangeAdd64(&header->UnderrunFrames, requestedFrames);
        return;
    }

    ULONG availableFrames = static_cast<ULONG>(writeFrame - readFrame);
    ULONG copiedFrames = min(requestedFrames, availableFrames);
    ULONG mask = header->CapacityFrames - 1;
    ULONG sourceFrame = static_cast<ULONG>(readFrame) & mask;
    ULONG firstFrames = min(copiedFrames, header->CapacityFrames - sourceFrame);
    SIZE_T firstBytes = static_cast<SIZE_T>(firstFrames) * frameBytes;
    RtlCopyMemory(Destination, Reader->Samples + sourceFrame * SLB_RING_CHANNELS, firstBytes);
    if (firstFrames < copiedFrames)
    {
        SIZE_T secondBytes = static_cast<SIZE_T>(copiedFrames - firstFrames) * frameBytes;
        RtlCopyMemory(Destination + firstBytes, Reader->Samples, secondBytes);
    }

    KeMemoryBarrier();
    InterlockedExchange64(&header->ReadFrame, readFrame + copiedFrames);
    InterlockedExchangeAdd64(&header->ConsumedFrames, copiedFrames);
    if (copiedFrames < requestedFrames)
    {
        InterlockedExchangeAdd64(&header->UnderrunFrames, requestedFrames - copiedFrames);
    }
}
