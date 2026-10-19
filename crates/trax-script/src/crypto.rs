use boa_engine::{
    js_string, Context, JsArgs, JsNativeError, JsResult, JsValue, NativeFunction,
};
use md5::{Digest as Md5Digest, Md5};
use sha2::{Sha256, Sha512};
use hmac::{Hmac, Mac};
use hmac::digest::KeyInit;

type HmacMd5 = Hmac<Md5>;
type HmacSha256 = Hmac<Sha256>;
type HmacSha512 = Hmac<Sha512>;

fn get_string_arg(args: &[JsValue], index: usize, ctx: &mut Context) -> JsResult<String> {
    args.get_or_undefined(index).to_string(ctx).map(|s| s.to_std_string_escaped())
}

pub fn md5(
    _this: &JsValue,
    args: &[JsValue],
    ctx: &mut Context,
) -> JsResult<JsValue> {
    let data = get_string_arg(args, 0, ctx)?;
    let mut hasher = Md5::new();
    hasher.update(data.as_bytes());
    let result = hasher.finalize();
    Ok(JsValue::from(js_string!(hex::encode(result))))
}

pub fn sha256(
    _this: &JsValue,
    args: &[JsValue],
    ctx: &mut Context,
) -> JsResult<JsValue> {
    let data = get_string_arg(args, 0, ctx)?;
    let mut hasher = Sha256::new();
    hasher.update(data.as_bytes());
    let result = hasher.finalize();
    Ok(JsValue::from(js_string!(hex::encode(result))))
}

pub fn sha512(
    _this: &JsValue,
    args: &[JsValue],
    ctx: &mut Context,
) -> JsResult<JsValue> {
    let data = get_string_arg(args, 0, ctx)?;
    let mut hasher = Sha512::new();
    hasher.update(data.as_bytes());
    let result = hasher.finalize();
    Ok(JsValue::from(js_string!(hex::encode(result))))
}

pub fn hmac_md5(
    _this: &JsValue,
    args: &[JsValue],
    ctx: &mut Context,
) -> JsResult<JsValue> {
    let key = get_string_arg(args, 0, ctx)?;
    let data = get_string_arg(args, 1, ctx)?;
    let mut mac = HmacMd5::new_from_slice(key.as_bytes())
        .map_err(|_| JsNativeError::error().with_message("invalid hmac key length"))?;
    mac.update(data.as_bytes());
    let result = mac.finalize().into_bytes();
    Ok(JsValue::from(js_string!(hex::encode(result))))
}

pub fn hmac_sha256(
    _this: &JsValue,
    args: &[JsValue],
    ctx: &mut Context,
) -> JsResult<JsValue> {
    let key = get_string_arg(args, 0, ctx)?;
    let data = get_string_arg(args, 1, ctx)?;
    let mut mac = HmacSha256::new_from_slice(key.as_bytes())
        .map_err(|_| JsNativeError::error().with_message("invalid hmac key length"))?;
    mac.update(data.as_bytes());
    let result = mac.finalize().into_bytes();
    Ok(JsValue::from(js_string!(hex::encode(result))))
}

pub fn hmac_sha512(
    _this: &JsValue,
    args: &[JsValue],
    ctx: &mut Context,
) -> JsResult<JsValue> {
    let key = get_string_arg(args, 0, ctx)?;
    let data = get_string_arg(args, 1, ctx)?;
    let mut mac = HmacSha512::new_from_slice(key.as_bytes())
        .map_err(|_| JsNativeError::error().with_message("invalid hmac key length"))?;
    mac.update(data.as_bytes());
    let result = mac.finalize().into_bytes();
    Ok(JsValue::from(js_string!(hex::encode(result))))
}

pub fn register(ctx: &mut Context) -> JsResult<()> {
    ctx.register_global_callable(js_string!("__traxCryptoMd5"), 1, NativeFunction::from_fn_ptr(md5))?;
    ctx.register_global_callable(js_string!("__traxCryptoSha256"), 1, NativeFunction::from_fn_ptr(sha256))?;
    ctx.register_global_callable(js_string!("__traxCryptoSha512"), 1, NativeFunction::from_fn_ptr(sha512))?;
    ctx.register_global_callable(js_string!("__traxCryptoHmacMd5"), 2, NativeFunction::from_fn_ptr(hmac_md5))?;
    ctx.register_global_callable(js_string!("__traxCryptoHmacSha256"), 2, NativeFunction::from_fn_ptr(hmac_sha256))?;
    ctx.register_global_callable(js_string!("__traxCryptoHmacSha512"), 2, NativeFunction::from_fn_ptr(hmac_sha512))?;

    Ok(())
}
