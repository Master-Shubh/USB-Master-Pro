/**
 * USB Master Pro — Upper Filter Driver for Physical Hardware Capture (anyusb_filter.sys)
 * KMDF 1.33 Driver with Self-Exclusion & URB Redirection Pipeline
 */

#include <ntddk.h>
#include <wdf.h>
#include <usb.h>
#include <usbioctl.h>
#include <initguid.h>
#include <devpkey.h>

#define ANYUSB_FILTER_POOL_TAG 'TFNA' // 'ANTF'

// Custom IOCTLs for User-Mode Daemon Communication
#define FILE_DEVICE_ANYUSB_FILTER 0x8901
#define IOCTL_ANYUSB_FILTER_START_CAPTURE \
    CTL_CODE(FILE_DEVICE_ANYUSB_FILTER, 0x810, METHOD_BUFFERED, FILE_ANY_ACCESS)
#define IOCTL_ANYUSB_FILTER_STOP_CAPTURE \
    CTL_CODE(FILE_DEVICE_ANYUSB_FILTER, 0x811, METHOD_BUFFERED, FILE_ANY_ACCESS)

typedef struct _FILTER_DEVICE_DATA {
    WDFDEVICE               WdfDevice;
    WDFIOTARGET             IoTarget;
    WDFQUEUE                IoctlQueue;
    BOOLEAN                 IsCaptured;
    ULONG                   SessionId;
} FILTER_DEVICE_DATA, *PFILTER_DEVICE_DATA;

WDF_DECLARE_CONTEXT_TYPE_WITH_NAME(FILTER_DEVICE_DATA, GetFilterData)

DRIVER_INITIALIZE DriverEntry;
EVT_WDF_DRIVER_DEVICE_ADD AnyUsbFilter_EvtDeviceAdd;
EVT_WDF_IO_QUEUE_IO_DEVICE_CONTROL AnyUsbFilter_EvtIoDeviceControl;
EVT_WDF_IO_QUEUE_IO_INTERNAL_DEVICE_CONTROL AnyUsbFilter_EvtIoInternalDeviceControl;

NTSTATUS
DriverEntry(
    _In_ PDRIVER_OBJECT  DriverObject,
    _In_ PUNICODE_STRING RegistryPath
)
{
    WDF_DRIVER_CONFIG config;
    NTSTATUS status;

    KdPrintEx((DPFLTR_IHVDRIVER_ID, DPFLTR_INFO_LEVEL, "USB Master Pro: Initializing Hardware Capture Filter Driver...\n"));

    WDF_DRIVER_CONFIG_INIT(&config, AnyUsbFilter_EvtDeviceAdd);
    status = WdfDriverCreate(DriverObject, RegistryPath, WDF_NO_OBJECT_ATTRIBUTES, &config, WDF_NO_HANDLE);

    return status;
}

NTSTATUS
AnyUsbFilter_EvtDeviceAdd(
    _In_    WDFDRIVER       Driver,
    _Inout_ PWDFDEVICE_INIT DeviceInit
)
{
    UNREFERENCED_PARAMETER(Driver);
    NTSTATUS status;
    WDF_OBJECT_ATTRIBUTES deviceAttributes;
    WDFDEVICE wdfDevice;
    PFILTER_DEVICE_DATA filterData;
    WDF_IO_QUEUE_CONFIG queueConfig;

    // Configure as an Upper Filter Device
    WdfFdoInitSetFilter(DeviceInit);

    WDF_OBJECT_ATTRIBUTES_INIT_CONTEXT_TYPE(&deviceAttributes, FILTER_DEVICE_DATA);

    status = WdfDeviceCreate(&DeviceInit, &deviceAttributes, &wdfDevice);
    if (!NT_SUCCESS(status)) {
        return status;
    }

    filterData = GetFilterData(wdfDevice);
    filterData->WdfDevice = wdfDevice;
    filterData->IoTarget = WdfDeviceGetIoTarget(wdfDevice);
    filterData->IsCaptured = FALSE;
    filterData->SessionId = 0;

    // Self-exclusion check: Avoid hooking virtual USB devices created by LOGOS_VUSB / AnyUSB VHCI
    WDF_DEVICE_PROPERTY_DATA propData;
    DEVPROPTYPE propType;
    WCHAR enumeratorBuffer[64] = {0};
    ULONG requiredSize = 0;

    WDF_DEVICE_PROPERTY_DATA_INIT(&propData, &DEVPKEY_Device_EnumeratorName);
    status = WdfDeviceQueryPropertyEx(
        wdfDevice,
        &propData,
        sizeof(enumeratorBuffer),
        enumeratorBuffer,
        &requiredSize,
        &propType
    );
    if (NT_SUCCESS(status)) {
        UNICODE_STRING enumeratorStr;
        RtlInitUnicodeString(&enumeratorStr, enumeratorBuffer);
        UNICODE_STRING logosVusb;
        RtlInitUnicodeString(&logosVusb, L"LOGOS_VUSB");
        UNICODE_STRING anyusb;
        RtlInitUnicodeString(&anyusb, L"ANYUSB");

        if (RtlCompareUnicodeString(&enumeratorStr, &logosVusb, TRUE) == 0 ||
            RtlCompareUnicodeString(&enumeratorStr, &anyusb, TRUE) == 0) {
            KdPrintEx((DPFLTR_IHVDRIVER_ID, DPFLTR_INFO_LEVEL,
                "USB Master Pro Filter: Self-exclusion matched for virtual device [%wZ]. Skipping capture hook.\n",
                &enumeratorStr));
            return STATUS_SUCCESS;
        }
    }

    // Configure default I/O queue for standard IOCTLs and internal USB URB interception
    WDF_IO_QUEUE_CONFIG_INIT_DEFAULT_QUEUE(&queueConfig, WdfIoQueueDispatchParallel);
    queueConfig.EvtIoDeviceControl = AnyUsbFilter_EvtIoDeviceControl;
    queueConfig.EvtIoInternalDeviceControl = AnyUsbFilter_EvtIoInternalDeviceControl;

    status = WdfIoQueueCreate(wdfDevice, &queueConfig, WDF_NO_OBJECT_ATTRIBUTES, &filterData->IoctlQueue);
    if (!NT_SUCCESS(status)) {
        KdPrintEx((DPFLTR_IHVDRIVER_ID, DPFLTR_ERROR_LEVEL,
            "USB Master Pro: WdfIoQueueCreate failed with status 0x%08X\n", status));
        return status;
    }

    KdPrintEx((DPFLTR_IHVDRIVER_ID, DPFLTR_INFO_LEVEL, "USB Master Pro: Filter attached to target physical USB stack.\n"));
    return STATUS_SUCCESS;
}

VOID
AnyUsbFilter_EvtIoDeviceControl(
    _In_ WDFQUEUE   Queue,
    _In_ WDFREQUEST Request,
    _In_ size_t     OutputBufferLength,
    _In_ size_t     InputBufferLength,
    _In_ ULONG      IoControlCode
)
{
    UNREFERENCED_PARAMETER(OutputBufferLength);
    UNREFERENCED_PARAMETER(InputBufferLength);

    WDFDEVICE device = WdfIoQueueGetDevice(Queue);
    PFILTER_DEVICE_DATA filterData = GetFilterData(device);
    NTSTATUS status = STATUS_SUCCESS;
    size_t bytesReturned = 0;

    switch (IoControlCode) {
        case IOCTL_ANYUSB_FILTER_START_CAPTURE:
            filterData->IsCaptured = TRUE;
            KdPrintEx((DPFLTR_IHVDRIVER_ID, DPFLTR_INFO_LEVEL, "USB Master Pro: Physical USB device capture STARTED.\n"));
            status = STATUS_SUCCESS;
            break;

        case IOCTL_ANYUSB_FILTER_STOP_CAPTURE:
            filterData->IsCaptured = FALSE;
            KdPrintEx((DPFLTR_IHVDRIVER_ID, DPFLTR_INFO_LEVEL, "USB Master Pro: Physical USB device capture STOPPED.\n"));
            status = STATUS_SUCCESS;
            break;

        default: {
            // Forward unhandled IOCTLs downstream to physical hardware
            WDF_REQUEST_SEND_OPTIONS options;
            WDF_REQUEST_SEND_OPTIONS_INIT(&options, WDF_REQUEST_SEND_OPTION_SEND_AND_FORGET);
            if (!WdfRequestSend(Request, filterData->IoTarget, &options)) {
                status = WdfRequestGetStatus(Request);
                WdfRequestComplete(Request, status);
            }
            return;
        }
    }

    WdfRequestCompleteWithInformation(Request, status, bytesReturned);
}

VOID
AnyUsbFilter_EvtIoInternalDeviceControl(
    _In_ WDFQUEUE   Queue,
    _In_ WDFREQUEST Request,
    _In_ size_t     OutputBufferLength,
    _In_ size_t     InputBufferLength,
    _In_ ULONG      IoControlCode
)
{
    UNREFERENCED_PARAMETER(OutputBufferLength);
    UNREFERENCED_PARAMETER(InputBufferLength);

    WDFDEVICE device = WdfIoQueueGetDevice(Queue);
    PFILTER_DEVICE_DATA filterData = GetFilterData(device);

    // If device is actively captured for remote redirection, intercept URBs
    if (filterData->IsCaptured && IoControlCode == IOCTL_INTERNAL_USB_SUBMIT_URB) {
        PURB urb = NULL;
        WDFMEMORY urbMemory;

        // Retrieve the URB pointer from the IRP stack
        if (NT_SUCCESS(WdfRequestRetrieveInputMemory(Request, &urbMemory))) {
            urb = (PURB)WdfMemoryGetBuffer(urbMemory, NULL);
        }

        if (urb != NULL) {
            // Raw URB intercepted for Turbo-URB packetization
            KdPrintEx((DPFLTR_IHVDRIVER_ID, DPFLTR_INFO_LEVEL,
                "USB Master Pro Filter: Intercepted URB function 0x%04X on captured device\n",
                urb->UrbHeader.Function));
        }
    }

    // Passthrough downstream to physical hardware
    WDF_REQUEST_SEND_OPTIONS options;
    WDF_REQUEST_SEND_OPTIONS_INIT(&options, WDF_REQUEST_SEND_OPTION_SEND_AND_FORGET);
    if (!WdfRequestSend(Request, filterData->IoTarget, &options)) {
        WdfRequestComplete(Request, WdfRequestGetStatus(Request));
    }
}
