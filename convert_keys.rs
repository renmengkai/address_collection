use std::fs::{File, read_to_string};
use std::io::{BufWriter, Write};
use secp256k1::{Secp256k1, SecretKey};
use sha3::{Keccak256, Digest};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 读取私钥文件
    let private_keys_content = read_to_string("privateKeys.txt")?;
    let private_keys: Vec<&str> = private_keys_content
        .lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .collect();

    println!("读取到 {} 个私钥", private_keys.len());

    // 创建CSV文件
    let output_file = File::create("keys_addresses.csv")?;
    let mut writer = BufWriter::new(output_file);
    
    // 写入CSV头部
    writeln!(writer, "私钥,地址")?;

    let secp = Secp256k1::new();
    let mut success_count = 0;
    let mut error_count = 0;

    // 处理每个私钥
    for (index, private_key_str) in private_keys.iter().enumerate() {
        match convert_private_key_to_address(&secp, private_key_str) {
            Ok(address) => {
                writeln!(writer, "{},{}", private_key_str, address)?;
                success_count += 1;
                println!("[{}/{}] 成功: {} -> {}", 
                    index + 1, private_keys.len(), private_key_str, address);
            }
            Err(e) => {
                error_count += 1;
                eprintln!("[{}/{}] 错误: {} - {}", 
                    index + 1, private_keys.len(), private_key_str, e);
            }
        }
    }

    writer.flush()?;
    
    println!("\n转换完成！");
    println!("成功: {} 个", success_count);
    println!("失败: {} 个", error_count);
    println!("结果已保存到: keys_addresses.csv");

    Ok(())
}

fn convert_private_key_to_address(
    secp: &Secp256k1<secp256k1::All>,
    private_key_str: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    // 移除可能的0x前缀
    let key_str = private_key_str.strip_prefix("0x").unwrap_or(private_key_str);
    
    // 解析私钥
    let key_bytes = hex::decode(key_str)
        .map_err(|e| format!("私钥十六进制解码失败: {}", e))?;
    
    if key_bytes.len() != 32 {
        return Err(format!("私钥长度错误: 期望32字节，实际{}字节", key_bytes.len()).into());
    }

    let secret_key = SecretKey::from_slice(&key_bytes)
        .map_err(|e| format!("私钥格式无效: {}", e))?;

    // 获取公钥（未压缩格式，65字节）
    let public_key = secp256k1::PublicKey::from_secret_key(secp, &secret_key);
    let public_key_bytes = public_key.serialize_uncompressed();

    // 计算地址：Keccak256(公钥[1..65])的后20字节
    let mut hasher = Keccak256::new();
    hasher.update(&public_key_bytes[1..65]);
    let hash = hasher.finalize();
    
    // 取后20字节作为地址
    let address_bytes = &hash[12..32];
    let address = format!("0x{}", hex::encode(address_bytes));

    Ok(address)
}
