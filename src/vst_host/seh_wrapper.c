#include <windows.h>

/*
 * SEH-protected COM method invocations with vtable indices
 * verified against vst3 crate 0.3.0 bindings.rs struct definitions.
 *
 * VTable layout (base classes are flattened via #[repr(C)]):
 *   FUnknownVtbl:           [0] queryInterface [1] addRef [2] release
 *   IPluginBaseVtbl:        base(FUnknown) + [3] initialize [4] terminate
 *   IPluginFactoryVtbl:     base(FUnknown) + [3] getFactoryInfo [4] countClasses
 *                            [5] getClassInfo [6] createInstance
 *   IComponentVtbl:         base(IPluginBase) + [5] getControllerClassId
 *                            [6] setIoMode [7] getBusCount [8] getBusInfo
 *                            [9] getRoutingInfo [10] activateBus [11] setActive
 *                            [12] setState [13] getState
 *   IAudioProcessorVtbl:    base(FUnknown) + [3] setBusArrangements
 *                            [4] getBusArrangement [5] canProcessSampleSize
 *                            [6] getLatencySamples [7] setupProcessing
 *                            [8] setProcessing [9] process [10] getTailSamples
 *   IEditControllerVtbl:    base(IPluginBase) + [5] setComponentState
 *                            [6] setState [7] getState [8] getParameterCount
 *                            [9] getParameterInfo [10] getParamStringByValue
 *                            [11] getParamValueByString [12] normalizedParamToPlain
 *                            [13] plainParamToNormalized [14] getParamNormalized
 *                            [15] setParamNormalized [16] setComponentHandler
 *                            [17] createView
 *   IPlugViewVtbl:          base(FUnknown) + [3] isPlatformTypeSupported
 *                            [4] attached [5] removed [6] onWheel
 *                            [7] onKeyDown [8] onKeyUp [9] getSize
 *                            [10] onSize [11] onFocus [12] setFrame
 *                            [13] canResize [14] checkSizeConstraint
 *
 * g_seh_depth is a thread-local counter incremented on entry to each
 * SEH-protected wrapper and decremented on exit.  The Rust VEH handler
 * reads it via seh_get_protected_depth(); when > 0 the VEH suppresses
 * logging because the SEH frame will catch the exception.
 */

#ifdef __cplusplus
extern "C" {
#endif

#define VST3_API __stdcall

static __declspec(thread) int g_seh_depth = 0;
static __declspec(thread) unsigned long g_last_seh_code = 0;
static __declspec(thread) void* g_last_seh_addr = 0;
static __declspec(thread) unsigned long long g_last_seh_rdi = 0;
static __declspec(thread) unsigned long long g_last_seh_rax = 0;
static __declspec(thread) unsigned long long g_last_seh_rdx = 0;

int seh_get_protected_depth(void) { return g_seh_depth; }
unsigned long seh_get_last_exception_code(void) { return g_last_seh_code; }
void* seh_get_last_exception_address(void) { return g_last_seh_addr; }
unsigned long long seh_get_last_exception_rdi(void) { return g_last_seh_rdi; }
unsigned long long seh_get_last_exception_rax(void) { return g_last_seh_rax; }
unsigned long long seh_get_last_exception_rdx(void) { return g_last_seh_rdx; }

/* ---- FUnknown [0-2] ---- */

long seh_call_query_interface(void* com_obj, const void* iid, void** obj)
{
    if (!com_obj) return -1;
    ++g_seh_depth;
    void** vtable = *(void***)com_obj;
    typedef long (VST3_API *Fn)(void*, const void*, void**);
    Fn fn = (Fn)vtable[0];
    long result = -1;
    __try { result = fn(com_obj, iid, obj); }
    __except (EXCEPTION_EXECUTE_HANDLER) { result = -2; }
    --g_seh_depth;
    return result;
}

/* ---- IPluginFactory (base: FUnknown) [3-6] ---- */

long seh_call_count_classes(void* com_obj)
{
    if (!com_obj) return -1;
    ++g_seh_depth;
    void** vtable = *(void***)com_obj;
    typedef long (VST3_API *Fn)(void*);
    Fn fn = (Fn)vtable[4];
    long result = -1;
    __try { result = fn(com_obj); }
    __except (EXCEPTION_EXECUTE_HANDLER) { result = -2; }
    --g_seh_depth;
    return result;
}

long seh_call_get_class_info(void* com_obj, int idx, void* info)
{
    if (!com_obj) return -1;
    ++g_seh_depth;
    void** vtable = *(void***)com_obj;
    typedef long (VST3_API *Fn)(void*, int, void*);
    Fn fn = (Fn)vtable[5];
    long result = -1;
    __try { result = fn(com_obj, idx, info); }
    __except (EXCEPTION_EXECUTE_HANDLER) { result = -2; }
    --g_seh_depth;
    return result;
}

long seh_call_create_instance(void* com_obj, const void* cid, const void* iid, void** obj)
{
    if (!com_obj) return -1;
    ++g_seh_depth;
    void** vtable = *(void***)com_obj;
    typedef long (VST3_API *Fn)(void*, const void*, const void*, void**);
    Fn fn = (Fn)vtable[6];
    long result = -1;
    __try { result = fn(com_obj, cid, iid, obj); }
    __except (EXCEPTION_EXECUTE_HANDLER) { result = -2; }
    --g_seh_depth;
    return result;
}

/* ---- IPluginBase (base: FUnknown) [3-4] ---- */

long seh_call_initialize(void* com_obj, void* context)
{
    if (!com_obj) return -1;
    ++g_seh_depth;
    void** vtable = *(void***)com_obj;
    typedef long (VST3_API *Fn)(void*, void*);
    Fn fn = (Fn)vtable[3];
    long result = -1;
    __try { result = fn(com_obj, context); }
    __except (EXCEPTION_EXECUTE_HANDLER) { result = -2; }
    --g_seh_depth;
    return result;
}

long seh_call_terminate(void* com_obj)
{
    if (!com_obj) return -1;
    ++g_seh_depth;
    void** vtable = *(void***)com_obj;
    typedef long (VST3_API *Fn)(void*);
    Fn fn = (Fn)vtable[4];
    long result = -1;
    __try { result = fn(com_obj); }
    __except (EXCEPTION_EXECUTE_HANDLER) { result = -2; }
    --g_seh_depth;
    return result;
}

/* ---- IComponent (base: IPluginBase[0-4]) [5-13] ---- */

long seh_call_get_controller_class_id(void* com_obj, void* class_id)
{
    if (!com_obj) return -1;
    ++g_seh_depth;
    void** vtable = *(void***)com_obj;
    typedef long (VST3_API *Fn)(void*, void*);
    Fn fn = (Fn)vtable[5];
    long result = -1;
    __try { result = fn(com_obj, class_id); }
    __except (EXCEPTION_EXECUTE_HANDLER) { result = -2; }
    --g_seh_depth;
    return result;
}

long seh_call_set_io_mode(void* com_obj, int mode)
{
    if (!com_obj) return -1;
    ++g_seh_depth;
    void** vtable = *(void***)com_obj;
    typedef long (VST3_API *Fn)(void*, int);
    Fn fn = (Fn)vtable[6];
    long result = -1;
    __try { result = fn(com_obj, mode); }
    __except (EXCEPTION_EXECUTE_HANDLER) { result = -2; }
    --g_seh_depth;
    return result;
}

long seh_call_get_bus_count(void* com_obj, int type, int dir)
{
    if (!com_obj) return -1;
    ++g_seh_depth;
    void** vtable = *(void***)com_obj;
    typedef long (VST3_API *Fn)(void*, int, int);
    Fn fn = (Fn)vtable[7];
    long result = -1;
    __try { result = fn(com_obj, type, dir); }
    __except (EXCEPTION_EXECUTE_HANDLER) { result = -2; }
    --g_seh_depth;
    return result;
}

long seh_call_activate_bus(void* com_obj, int type, int dir, int idx, unsigned char state)
{
    if (!com_obj) return -1;
    ++g_seh_depth;
    void** vtable = *(void***)com_obj;
    typedef long (VST3_API *Fn)(void*, int, int, int, unsigned char);
    Fn fn = (Fn)vtable[10];
    long result = -1;
    __try { result = fn(com_obj, type, dir, idx, state); }
    __except (EXCEPTION_EXECUTE_HANDLER) { result = -2; }
    --g_seh_depth;
    return result;
}

long seh_call_set_active(void* com_obj, unsigned char state)
{
    if (!com_obj) return -1;
    ++g_seh_depth;
    void** vtable = *(void***)com_obj;
    typedef long (VST3_API *Fn)(void*, unsigned char);
    Fn fn = (Fn)vtable[11];
    long result = -1;
    __try { result = fn(com_obj, state); }
    __except (EXCEPTION_EXECUTE_HANDLER) { result = -2; }
    --g_seh_depth;
    return result;
}

long seh_call_component_get_state(void* com_obj, void* state)
{
    if (!com_obj) return -1;
    ++g_seh_depth;
    void** vtable = *(void***)com_obj;
    typedef long (VST3_API *Fn)(void*, void*);
    Fn fn = (Fn)vtable[13];
    long result = -1;
    __try { result = fn(com_obj, state); }
    __except (EXCEPTION_EXECUTE_HANDLER) { result = -2; }
    --g_seh_depth;
    return result;
}

long seh_call_component_set_state(void* com_obj, void* state)
{
    if (!com_obj) return -1;
    ++g_seh_depth;
    void** vtable = *(void***)com_obj;
    typedef long (VST3_API *Fn)(void*, void*);
    Fn fn = (Fn)vtable[12];
    long result = -1;
    __try { result = fn(com_obj, state); }
    __except (EXCEPTION_EXECUTE_HANDLER) { result = -2; }
    --g_seh_depth;
    return result;
}

/* ---- IAudioProcessor (base: FUnknown[0-2]) [3-10] ---- */

long seh_call_set_bus_arrangements(void* com_obj, void* inputs, int num_ins, void* outputs, int num_outs)
{
    if (!com_obj) return -1;
    ++g_seh_depth;
    void** vtable = *(void***)com_obj;
    typedef long (VST3_API *Fn)(void*, void*, int, void*, int);
    Fn fn = (Fn)vtable[3];
    long result = -1;
    __try { result = fn(com_obj, inputs, num_ins, outputs, num_outs); }
    __except (EXCEPTION_EXECUTE_HANDLER) { result = -2; }
    --g_seh_depth;
    return result;
}

long seh_call_setup_processing(void* com_obj, void* setup)
{
    if (!com_obj) return -1;
    ++g_seh_depth;
    void** vtable = *(void***)com_obj;
    typedef long (VST3_API *Fn)(void*, void*);
    Fn fn = (Fn)vtable[7];
    long result = -1;
    __try { result = fn(com_obj, setup); }
    __except (EXCEPTION_EXECUTE_HANDLER) { result = -2; }
    --g_seh_depth;
    return result;
}

long seh_call_set_processing(void* com_obj, unsigned char state)
{
    if (!com_obj) return -1;
    ++g_seh_depth;
    void** vtable = *(void***)com_obj;
    typedef long (VST3_API *Fn)(void*, unsigned char);
    Fn fn = (Fn)vtable[8];
    long result = -1;
    __try { result = fn(com_obj, state); }
    __except (EXCEPTION_EXECUTE_HANDLER) { result = -2; }
    --g_seh_depth;
    return result;
}

long seh_call_process_robust(void* com_obj, int num_samples, int num_inputs, void* inputs, int num_outputs, void* outputs, void* context)
{
    if (!com_obj) return -1;
    ++g_seh_depth;

    struct {
        int processMode;
        int symbolicSampleSize;
        int numSamples;
        int numInputs;
        int numOutputs;
        void* inputs;
        void* outputs;
        void* inputParameterChanges;
        void* outputParameterChanges;
        void* inputEvents;
        void* outputEvents;
        void* processContext;
    } data;

    data.processMode = 0;
    data.symbolicSampleSize = 0;
    data.numSamples = num_samples;
    data.numInputs = num_inputs;
    data.numOutputs = num_outputs;
    data.inputs = inputs;
    data.outputs = outputs;
    data.inputParameterChanges = 0;
    data.outputParameterChanges = 0;
    data.inputEvents = 0;
    data.outputEvents = 0;
    data.processContext = context;

    void** vtable = *(void***)com_obj;
    typedef long (VST3_API *Fn)(void*, void*);
    Fn fn = (Fn)vtable[9];
    long result = -1;
    __try { result = fn(com_obj, &data); }
    __except (EXCEPTION_EXECUTE_HANDLER) { result = -2; }
    --g_seh_depth;
    return result;
}

/* ---- IEditController (base: IPluginBase[0-4]) [5-17] ---- */

long seh_call_set_component_state(void* com_obj, void* state)
{
    if (!com_obj) return -1;
    ++g_seh_depth;
    void** vtable = *(void***)com_obj;
    typedef long (VST3_API *Fn)(void*, void*);
    Fn fn = (Fn)vtable[5];
    long result = -1;
    __try { result = fn(com_obj, state); }
    __except (EXCEPTION_EXECUTE_HANDLER) { result = -2; }
    --g_seh_depth;
    return result;
}

long seh_call_set_component_handler(void* com_obj, void* handler)
{
    if (!com_obj) return -1;
    ++g_seh_depth;
    void** vtable = *(void***)com_obj;
    typedef long (VST3_API *Fn)(void*, void*);
    Fn fn = (Fn)vtable[16];
    long result = -1;
    __try { result = fn(com_obj, handler); }
    __except (EXCEPTION_EXECUTE_HANDLER) { result = -2; }
    --g_seh_depth;
    return result;
}

long seh_call_get_parameter_count(void* com_obj)
{
    if (!com_obj) return -1;
    ++g_seh_depth;
    void** vtable = *(void***)com_obj;
    typedef long (VST3_API *Fn)(void*);
    Fn fn = (Fn)vtable[8];
    long result = -1;
    __try { result = fn(com_obj); }
    __except (EXCEPTION_EXECUTE_HANDLER) { result = -2; }
    --g_seh_depth;
    return result;
}

long seh_call_get_parameter_info(void* com_obj, int idx, void* info)
{
    if (!com_obj) return -1;
    ++g_seh_depth;
    void** vtable = *(void***)com_obj;
    typedef long (VST3_API *Fn)(void*, int, void*);
    Fn fn = (Fn)vtable[9];
    long result = -1;
    __try { result = fn(com_obj, idx, info); }
    __except (EXCEPTION_EXECUTE_HANDLER) { result = -2; }
    --g_seh_depth;
    return result;
}

double seh_call_get_param_normalized(void* com_obj, unsigned int id)
{
    if (!com_obj) return 0.0;
    ++g_seh_depth;
    void** vtable = *(void***)com_obj;
    typedef double (VST3_API *Fn)(void*, unsigned int);
    Fn fn = (Fn)vtable[14];
    double result = 0.0;
    __try { result = fn(com_obj, id); }
    __except (EXCEPTION_EXECUTE_HANDLER) { result = 0.0; }
    --g_seh_depth;
    return result;
}

long seh_call_set_param_normalized(void* com_obj, unsigned int id, double value)
{
    if (!com_obj) return -1;
    ++g_seh_depth;
    void** vtable = *(void***)com_obj;
    typedef long (VST3_API *Fn)(void*, unsigned int, double);
    Fn fn = (Fn)vtable[15];
    long result = -1;
    __try { result = fn(com_obj, id, value); }
    __except (EXCEPTION_EXECUTE_HANDLER) { result = -2; }
    --g_seh_depth;
    return result;
}

void* seh_call_create_view(void* com_obj, const char* name)
{
    if (!com_obj) return (void*)0;
    ++g_seh_depth;
    void** vtable = *(void***)com_obj;
    typedef void* (VST3_API *Fn)(void*, const char*);
    Fn fn = (Fn)vtable[17];
    void* result = 0;
    __try { result = fn(com_obj, name); }
    __except (EXCEPTION_EXECUTE_HANDLER) { result = (void*)-2; }
    --g_seh_depth;
    return result;
}

/* ---- IConnectionPoint (base: FUnknown[0-2]) [3-5] ---- */

long seh_call_connection_point_connect(void* com_obj, void* other)
{
    if (!com_obj) return -1;
    ++g_seh_depth;
    void** vtable = *(void***)com_obj;
    typedef long (VST3_API *Fn)(void*, void*);
    Fn fn = (Fn)vtable[3];
    long result = -1;
    __try { result = fn(com_obj, other); }
    __except (EXCEPTION_EXECUTE_HANDLER) { result = -2; }
    --g_seh_depth;
    return result;
}

long seh_call_connection_point_disconnect(void* com_obj, void* other)
{
    if (!com_obj) return -1;
    ++g_seh_depth;
    void** vtable = *(void***)com_obj;
    typedef long (VST3_API *Fn)(void*, void*);
    Fn fn = (Fn)vtable[4];
    long result = -1;
    __try { result = fn(com_obj, other); }
    __except (EXCEPTION_EXECUTE_HANDLER) { result = -2; }
    --g_seh_depth;
    return result;
}

/* ---- IPlugView (base: FUnknown[0-2]) [3-14] ---- */

long seh_call_is_platform_type_supported(void* com_obj, const char* type)
{
    if (!com_obj) return -1;
    ++g_seh_depth;
    void** vtable = *(void***)com_obj;
    typedef long (VST3_API *Fn)(void*, const char*);
    Fn fn = (Fn)vtable[3];
    long result = -1;
    __try { result = fn(com_obj, type); }
    __except (EXCEPTION_EXECUTE_HANDLER) { result = -2; }
    --g_seh_depth;
    return result;
}

static long __cdecl seh_capture_filter(EXCEPTION_POINTERS* ep)
{
    g_last_seh_code = ep->ExceptionRecord->ExceptionCode;
    g_last_seh_addr = ep->ExceptionRecord->ExceptionAddress;
    if (ep->ContextRecord) {
#ifdef _WIN64
        g_last_seh_rdi = ep->ContextRecord->Rdi;
        g_last_seh_rax = ep->ContextRecord->Rax;
        g_last_seh_rdx = ep->ContextRecord->Rdx;
#else
        g_last_seh_rdi = 0;
        g_last_seh_rax = 0;
        g_last_seh_rdx = 0;
#endif
    }
    return EXCEPTION_EXECUTE_HANDLER;
}

long seh_call_plug_view_attached(void* com_obj, void* parent, const char* type)
{
    if (!com_obj) return -1;
    ++g_seh_depth;
    g_last_seh_code = 0;
    g_last_seh_addr = 0;
    g_last_seh_rdi = 0;
    g_last_seh_rax = 0;
    g_last_seh_rdx = 0;
    void** vtable = *(void***)com_obj;
    typedef long (VST3_API *Fn)(void*, void*, const char*);
    Fn fn = (Fn)vtable[4];
    long result = -1;
    __try { result = fn(com_obj, parent, type); }
    __except (seh_capture_filter(GetExceptionInformation())) { result = -2; }
    --g_seh_depth;
    return result;
}

long seh_call_plug_view_removed(void* com_obj)
{
    if (!com_obj) return -1;
    ++g_seh_depth;
    void** vtable = *(void***)com_obj;
    typedef long (VST3_API *Fn)(void*);
    Fn fn = (Fn)vtable[5];
    long result = -1;
    __try { result = fn(com_obj); }
    __except (EXCEPTION_EXECUTE_HANDLER) { result = -2; }
    --g_seh_depth;
    return result;
}

long seh_call_plug_view_get_size(void* com_obj, void* rect)
{
    if (!com_obj) return -1;
    ++g_seh_depth;
    void** vtable = *(void***)com_obj;
    typedef long (VST3_API *Fn)(void*, void*);
    Fn fn = (Fn)vtable[9];
    long result = -1;
    __try { result = fn(com_obj, rect); }
    __except (EXCEPTION_EXECUTE_HANDLER) { result = -2; }
    --g_seh_depth;
    return result;
}

long seh_call_plug_view_set_frame(void* com_obj, void* frame)
{
    if (!com_obj) return -1;
    ++g_seh_depth;
    void** vtable = *(void***)com_obj;
    typedef long (VST3_API *Fn)(void*, void*);
    Fn fn = (Fn)vtable[12];
    long result = -1;
    __try { result = fn(com_obj, frame); }
    __except (EXCEPTION_EXECUTE_HANDLER) { result = -2; }
    --g_seh_depth;
    return result;
}

long seh_call_set_content_scale_factor(void* com_obj, float factor)
{
    if (!com_obj) return -1;
    ++g_seh_depth;
    void** vtable = *(void***)com_obj;
    typedef long (VST3_API *Fn)(void*, float);
    Fn fn = (Fn)vtable[3];
    long result = -1;
    __try { result = fn(com_obj, factor); }
    __except (EXCEPTION_EXECUTE_HANDLER) { result = -2; }
    --g_seh_depth;
    return result;
}

#ifdef __cplusplus
}
#endif
