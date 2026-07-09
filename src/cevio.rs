/// CeVIO AI COM連携モジュール
/// windows-core::VARIANT の From/TryFrom を活用したIDispatch操作
use std::collections::HashMap;
use tokio::sync::mpsc;

use windows::Win32::System::Com::{
    CLSIDFromProgID, CoCreateInstance, CoInitializeEx, CoUninitialize, IDispatch,
    COINIT_APARTMENTTHREADED, DISPATCH_METHOD,
    DISPATCH_PROPERTYGET, DISPATCH_PROPERTYPUT, DISPPARAMS,
};
use windows::core::{BSTR, GUID, PCWSTR, VARIANT};

const DISPID_PROPERTYPUT: i32 = -3;

// ============================================================
// 公開API
// ============================================================

pub enum CevioCommand {
    Speak { text: String },
    UpdateParams(CevioParams),
    Stop,
}

#[derive(Debug, Clone)]
pub struct CevioParams {
    pub narrator: Option<String>,
    pub speed: u32,
    pub pitch: u32,
    pub volume: u32,
    pub alpha: u32,
    pub intonation: u32,
    pub emotions: HashMap<String, u32>,
    pub skip_threshold: u32, // 0 = 無効
}

impl Default for CevioParams {
    fn default() -> Self {
        Self {
            narrator: None,
            speed: 50,
            pitch: 50,
            volume: 50,
            alpha: 50,
            intonation: 50,
            emotions: HashMap::new(),
            skip_threshold: 0,
        }
    }
}

pub fn get_narrators() -> Vec<String> {
    std::thread::spawn(|| unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let r = get_narrators_com().unwrap_or_default();
        CoUninitialize();
        r
    })
    .join()
    .unwrap_or_default()
}

pub fn get_emotions(narrator: &str) -> Vec<String> {
    let narrator = narrator.to_string();
    std::thread::spawn(move || unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let r = get_emotions_com(&narrator).unwrap_or_default();
        CoUninitialize();
        r
    })
    .join()
    .unwrap_or_default()
}

pub fn start_cevio_thread(params: CevioParams) -> mpsc::Sender<CevioCommand> {
    let (tx, mut rx) = mpsc::channel::<CevioCommand>(64);

    std::thread::spawn(move || {
        unsafe { 
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED); 
            eprintln!("DEBUG: CeVIO thread started");
        }

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokioランタイム作成失敗");

        rt.block_on(async move {
            let mut cur_params = params;
            let mut speech_queue: std::collections::VecDeque<String> = std::collections::VecDeque::new();
            let mut last_state: Option<IDispatch> = None;
            
            let talker = unsafe {
                match create_dispatch("CeVIO.Talk.RemoteService2.Talker2")
                    .or_else(|_| create_dispatch("CeVIO.Talk.RemoteService2.Talker2V40")) {
                    Ok(t) => Some(t),
                    Err(_) => None
                }
            };

            loop {
                tokio::select! {
                    cmd_opt = rx.recv() => {
                        match cmd_opt {
                            Some(CevioCommand::Speak { text }) => {
                                speech_queue.push_back(text);
                                
                                // キューが溜まりすぎていたらスキップ
                                if cur_params.skip_threshold > 0 && speech_queue.len() > cur_params.skip_threshold as usize {
                                    let excess = speech_queue.len() - cur_params.skip_threshold as usize;
                                    for _ in 0..excess {
                                        speech_queue.pop_front();
                                    }
                                    eprintln!("DEBUG: Queue too long, skipped {} messages", excess);
                                }
                            },
                            Some(CevioCommand::UpdateParams(p)) => {
                                cur_params = p;
                            },
                            Some(CevioCommand::Stop) => {
                                speech_queue.clear();
                                if let Some(ref t) = talker {
                                    unsafe {
                                        let mut no_args: Vec<VARIANT> = vec![];
                                        let _ = call_method(t, "Stop", &mut no_args);
                                        last_state = None;
                                    }
                                }
                            },
                            None => break,
                        }
                    },
                    // 定期チェック (50ms)
                    _ = tokio::time::sleep(std::time::Duration::from_millis(50)) => {
                        unsafe {
                            // 1. 現在の読み上げが終わったかチェック
                            if let Some(ref state) = last_state {
                                if let Ok(is_comp_var) = get_prop(state, "IsCompleted") {
                                    if let Ok(completed) = bool::try_from(&is_comp_var) {
                                        if completed {
                                            last_state = None;
                                        }
                                    }
                                }
                            }

                            // 2. 読み上げ中でなく、キューにメッセージがあれば開始
                            if last_state.is_none() {
                                if let Some(text) = speech_queue.pop_front() {
                                    if let Some(ref t) = talker {
                                        let _ = set_params_only(t, &cur_params);
                                        let mut args = [VARIANT::from(BSTR::from(text.as_str()))];
                                        if let Ok(v) = call_method(t, "Speak", &mut args) {
                                            last_state = variant_to_dispatch(v).ok();
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        });

        unsafe { CoUninitialize() };
        eprintln!("DEBUG: CeVIO thread stopped");
    });

    tx
}

unsafe fn set_params_only(talker: &IDispatch, params: &CevioParams) -> windows::core::Result<()> {
    if let Some(ref n) = params.narrator {
        let _ = set_prop(talker, "Cast", VARIANT::from(BSTR::from(n.as_str())));
    }
    let _ = set_prop(talker, "Speed", VARIANT::from(params.speed));
    let _ = set_prop(talker, "Tone", VARIANT::from(params.pitch));
    let _ = set_prop(talker, "Volume", VARIANT::from(params.volume));
    let _ = set_prop(talker, "Alpha", VARIANT::from(params.alpha));
    let _ = set_prop(talker, "ToneScale", VARIANT::from(params.intonation));

    // 感情設定
    if !params.emotions.is_empty() {
        if let Ok(comps_var) = get_prop(talker, "Components") {
            if let Ok(comps) = variant_to_dispatch(comps_var) {
                if let Ok(len_var) = get_prop(&comps, "Length") {
                    if let Ok(len) = i32::try_from(&len_var) {
                        for i in 0..len as usize {
                            let mut idx = [VARIANT::from(i as i32)];
                            if let Ok(comp_var) = call_method(&comps, "At", &mut idx) {
                                if let Ok(comp) = variant_to_dispatch(comp_var) {
                                    if let Ok(name_var) = get_prop(&comp, "Name") {
                                        if let Ok(name) = BSTR::try_from(&name_var) {
                                            let name_str = name.to_string();
                                            if let Some(&val) = params.emotions.get(&name_str) {
                                                let _ = set_prop(&comp, "Value", VARIANT::from(val));
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
    Ok(())
}

// ============================================================
// COM低レベルヘルパー
// ============================================================

unsafe fn create_instance_by_clsid(clsid_str: &str) -> windows::core::Result<IDispatch> {
    // "{...}" 形式の文字列から GUID を作成
    let wide: Vec<u16> = clsid_str.encode_utf16().chain(std::iter::once(0)).collect();
    let clsid = windows::Win32::System::Com::CLSIDFromString(PCWSTR(wide.as_ptr()))?;
    CoCreateInstance(&clsid, None, windows::Win32::System::Com::CLSCTX_ALL)
}

unsafe fn create_dispatch(progid: &str) -> windows::core::Result<IDispatch> {
    match progid {
        "CeVIO.Talk.RemoteService2.ServiceControl2" => create_instance_by_clsid("{B75AFE5E-DD52-42E9-A6F5-7BE5F6FE8EDB}"),
        "CeVIO.Talk.RemoteService2.Talker2" => create_instance_by_clsid("{EFBCD077-659B-4E6B-A8A9-FE88BE66308C}"),
        _ => {
            let wide: Vec<u16> = progid.encode_utf16().chain(std::iter::once(0)).collect();
            let clsid = CLSIDFromProgID(PCWSTR(wide.as_ptr()))?;
            CoCreateInstance(&clsid, None, windows::Win32::System::Com::CLSCTX_ALL)
        }
    }
}

unsafe fn get_dispid(d: &IDispatch, name: &str) -> windows::core::Result<i32> {
    let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
    let mut dispid: i32 = 0;
    d.GetIDsOfNames(&GUID::zeroed(), &PCWSTR(wide.as_ptr()), 1, 0x0409, &mut dispid)?;
    Ok(dispid)
}

unsafe fn get_prop(d: &IDispatch, name: &str) -> windows::core::Result<VARIANT> {
    let dispid = get_dispid(d, name)?;
    let mut result = VARIANT::default();
    let params = DISPPARAMS { rgvarg: std::ptr::null_mut(), rgdispidNamedArgs: std::ptr::null_mut(), cArgs: 0, cNamedArgs: 0 };
    d.Invoke(dispid, &GUID::zeroed(), 0, DISPATCH_PROPERTYGET, &params, Some(&mut result), None, None)?;
    Ok(result)
}

unsafe fn set_prop(d: &IDispatch, name: &str, mut arg: VARIANT) -> windows::core::Result<()> {
    let dispid = get_dispid(d, name)?;
    let mut named = DISPID_PROPERTYPUT;
    let params = DISPPARAMS { rgvarg: &mut arg, rgdispidNamedArgs: &mut named, cArgs: 1, cNamedArgs: 1 };
    d.Invoke(dispid, &GUID::zeroed(), 0, DISPATCH_PROPERTYPUT, &params, None, None, None)?;
    Ok(())
}

unsafe fn call_method(d: &IDispatch, name: &str, args: &mut [VARIANT]) -> windows::core::Result<VARIANT> {
    let dispid = get_dispid(d, name)?;
    let mut result = VARIANT::default();
    // COMはargsを逆順で渡す
    args.reverse();
    let params = DISPPARAMS {
        rgvarg: if args.is_empty() { std::ptr::null_mut() } else { args.as_mut_ptr() },
        rgdispidNamedArgs: std::ptr::null_mut(),
        cArgs: args.len() as u32,
        cNamedArgs: 0,
    };
    d.Invoke(dispid, &GUID::zeroed(), 0, DISPATCH_METHOD, &params, Some(&mut result), None, None)?;
    Ok(result)
}

unsafe fn variant_to_dispatch(v: VARIANT) -> windows::core::Result<IDispatch> {
    use windows::core::Interface;
    let raw = v.as_raw();
    if raw.Anonymous.Anonymous.vt == 9u16 {
        let p = raw.Anonymous.Anonymous.Anonymous.pdispVal
            as *mut std::ffi::c_void;
        if !p.is_null() {
            // from_rawは所有権を取る（Release担当）、cloneはAddRef+Release
            let dispatch = IDispatch::from_raw(p);
            let cloned = dispatch.clone();
            // from_rawで所有権を取ったdispatchをreleaseしないようにinto_rawで回収
            dispatch.into_raw();
            return Ok(cloned);
        }
    }
    Err(windows::core::Error::from_win32())
}

// ============================================================
// 高レベルCOM操作
// ============================================================

unsafe fn get_narrators_com() -> windows::core::Result<Vec<String>> {
    eprintln!("DEBUG: Starting get_narrators_com");
    
    // 1. サービスの開始を確認
    let svc = match create_dispatch("CeVIO.Talk.RemoteService2.ServiceControl2")
        .or_else(|_| create_dispatch("CeVIO.Talk.RemoteService2.ServiceControl2V40")) {
        Ok(s) => {
            eprintln!("DEBUG: ServiceControl created");
            s
        },
        Err(e) => {
            eprintln!("DEBUG: Failed to create ServiceControl: {:?}", e);
            return Err(e);
        }
    };
    
    let mut args = [VARIANT::from(false)];
    match call_method(&svc, "StartHost", &mut args) {
        Ok(_) => eprintln!("DEBUG: StartHost called"),
        Err(e) => eprintln!("DEBUG: StartHost failed (ignoring): {:?}", e),
    }

    // 2. Talkerの作成
    let talker = match create_dispatch("CeVIO.Talk.RemoteService2.Talker2")
        .or_else(|_| create_dispatch("CeVIO.Talk.RemoteService2.Talker2V40")) {
        Ok(t) => {
            eprintln!("DEBUG: Talker created");
            t
        },
        Err(e) => {
            eprintln!("DEBUG: Failed to create Talker: {:?}", e);
            return Err(e);
        }
    };

    let casts_var = get_prop(&talker, "AvailableCasts")?;
    eprintln!("DEBUG: AvailableCasts prop got, vt={}", casts_var.as_raw().Anonymous.Anonymous.vt);
    
    let casts = variant_to_dispatch(casts_var)?;
    eprintln!("DEBUG: CastCollection dispatch created");
    
    let len_var = get_prop(&casts, "Length")?;
    let len = i32::try_from(&len_var).unwrap_or(0) as usize;
    eprintln!("DEBUG: Casts count = {}", len);

    let mut result = Vec::new();
    for i in 0..len {
        let mut idx = [VARIANT::from(i as i32)];
        if let Ok(item) = call_method(&casts, "At", &mut idx) {
            if let Ok(s) = BSTR::try_from(&item) {
                result.push(s.to_string());
            }
        }
    }
    eprintln!("DEBUG: Narrators retrieval finished, count={}", result.len());
    Ok(result)
}

unsafe fn get_emotions_com(narrator: &str) -> windows::core::Result<Vec<String>> {
    let talker = create_dispatch("CeVIO.Talk.RemoteService2.Talker2")
        .or_else(|_| create_dispatch("CeVIO.Talk.RemoteService2.Talker2V40"))?;
    set_prop(&talker, "Cast", VARIANT::from(BSTR::from(narrator)))?;
    let comps_var = get_prop(&talker, "Components")?;
    let comps = variant_to_dispatch(comps_var)?;
    let len = i32::try_from(&get_prop(&comps, "Length")?)? as usize;

    let mut result = Vec::new();
    for i in 0..len {
        let mut idx = [VARIANT::from(i as i32)];
        if let Ok(comp_var) = call_method(&comps, "At", &mut idx) {
            if let Ok(comp) = variant_to_dispatch(comp_var) {
                if let Ok(name_var) = get_prop(&comp, "Name") {
                    if let Ok(s) = BSTR::try_from(&name_var) {
                        result.push(s.to_string());
                    }
                }
            }
        }
    }
    Ok(result)
}


