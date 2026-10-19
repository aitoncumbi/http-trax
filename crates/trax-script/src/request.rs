use boa_engine::{
    js_string, Context, JsArgs, JsNativeError, JsResult, JsValue, NativeFunction, JsObject,
    object::FunctionObjectBuilder,
};
use std::collections::BTreeMap;

pub fn send_request(
    _this: &JsValue,
    args: &[JsValue],
    ctx: &mut Context,
) -> JsResult<JsValue> {
    let req_obj = args.get_or_undefined(0);
    let mut url = String::new();
    let mut method = "GET".to_string();
    let mut headers = BTreeMap::new();
    let mut body = None;

    if let Some(req_str) = req_obj.as_string() {
        url = req_str.to_std_string_escaped();
    } else if let Some(obj) = req_obj.as_object() {
        if let Ok(u) = obj.get(js_string!("url"), ctx) {
            if let Some(u_str) = u.as_string() {
                url = u_str.to_std_string_escaped();
            }
        }
        if let Ok(m) = obj.get(js_string!("method"), ctx) {
            if let Some(m_str) = m.as_string() {
                method = m_str.to_std_string_escaped();
            }
        }
        if let Ok(h) = obj.get(js_string!("header"), ctx) {
            if let Some(h_obj) = h.as_object() {
                let keys = h_obj.own_property_keys(ctx)?;
                for key in keys {
                    let key_clone = key.clone();
                    if let Ok(key_str_val) = JsValue::from(key_clone).to_string(ctx) {
                        let key_str = key_str_val.to_std_string_escaped();
                        if let Ok(val) = h_obj.get(key, ctx) {
                            if let Ok(val_str_val) = val.to_string(ctx) {
                                let val_str = val_str_val.to_std_string_escaped();
                                headers.insert(key_str, val_str);
                            }
                        }
                    }
                }
            }
        }
        if let Ok(b) = obj.get(js_string!("body"), ctx) {
            if let Some(b_obj) = b.as_object() {
                if let Ok(raw) = b_obj.get(js_string!("raw"), ctx) {
                    if let Some(raw_str) = raw.as_string() {
                        body = Some(raw_str.to_std_string_escaped());
                    }
                }
            } else if let Some(b_str) = b.as_string() {
                body = Some(b_str.to_std_string_escaped());
            }
        }
    } else {
        return Err(JsNativeError::typ().with_message("expected request string or object").into());
    }

    let cb_opt = args.get_or_undefined(1).as_callable().map(|c| c.clone());

    let client = reqwest::blocking::Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .map_err(|e| JsNativeError::error().with_message(format!("client error: {}", e)))?;

    let method = match method.to_uppercase().as_str() {
        "GET" => reqwest::Method::GET,
        "POST" => reqwest::Method::POST,
        "PUT" => reqwest::Method::PUT,
        "DELETE" => reqwest::Method::DELETE,
        "PATCH" => reqwest::Method::PATCH,
        _ => reqwest::Method::GET,
    };

    let mut req_builder = client.request(method, &url);
    for (k, v) in headers {
        req_builder = req_builder.header(&k, &v);
    }
    if let Some(b) = body {
        req_builder = req_builder.body(b);
    }

    let resp_res = req_builder.send();

    if let Some(callback) = cb_opt {
        match resp_res {
            Ok(resp) => {
                let status = resp.status();
                let text = resp.text().unwrap_or_default();

                let obj = JsObject::with_object_proto(ctx.intrinsics());
                let _ = obj.set(js_string!("code"), JsValue::from(status.as_u16()), false, ctx);
                let _ = obj.set(js_string!("status"), JsValue::from(js_string!(status.canonical_reason().unwrap_or(""))), false, ctx);
                let _ = obj.set(js_string!("text"), JsValue::from(js_string!(text.clone())), false, ctx);
                let _ = obj.set(js_string!("stream"), JsValue::from(js_string!(text.clone())), false, ctx);

                let json_text = text.clone();
                let json_fn = NativeFunction::from_copy_closure_with_captures(
                    move |_this: &JsValue, _args: &[JsValue], captures: &String, _ctx: &mut Context| -> JsResult<JsValue> {
                        // For a real `pm.sendRequest` json() should parse the text into a JSON object.
                        // We do a simple JSON.parse
                        let parsed = serde_json::from_str::<serde_json::Value>(captures.as_str())
                            .unwrap_or(serde_json::Value::Null);
                        let json_str = serde_json::to_string(&parsed).unwrap_or_default();
                        let script = format!("JSON.parse('{}')", json_str.replace("'", "\\'"));
                        let val = _ctx.eval(boa_engine::Source::from_bytes(script.as_bytes()))?;
                        Ok(val)
                    },
                    json_text,
                );

                let json_fn_obj = FunctionObjectBuilder::new(ctx.realm(), json_fn).build();
                let _ = obj.set(js_string!("json"), JsValue::from(json_fn_obj), false, ctx);

                let args_for_call: [JsValue; 2] = [JsValue::undefined(), JsValue::from(obj)];
                let _ = JsObject::call(&callback, &JsValue::undefined(), &args_for_call, ctx)?;
                Ok(JsValue::undefined())
            },
            Err(e) => {
                let err_obj = JsObject::with_object_proto(ctx.intrinsics());
                let _ = err_obj.set(js_string!("message"), JsValue::from(js_string!(e.to_string())), false, ctx);
                let args_for_call: [JsValue; 2] = [JsValue::from(err_obj), JsValue::undefined()];
                let _ = JsObject::call(&callback, &JsValue::undefined(), &args_for_call, ctx)?;
                Ok(JsValue::undefined())
            }
        }
    } else {
        Ok(JsValue::undefined())
    }
}

pub fn register(ctx: &mut Context) -> JsResult<()> {
    ctx.register_global_callable(js_string!("__traxSendRequest"), 2, NativeFunction::from_fn_ptr(send_request))?;
    Ok(())
}
