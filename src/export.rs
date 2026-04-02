use crate::blockchain::{QueryResult, QueryStatus};
use crate::error::{BlockchainError, Result};
use csv::Writer;
use rust_xlsxwriter::{Workbook, Format, FormatBorder};
use std::collections::HashMap;
use std::fs::File;
use std::path::Path;

pub struct CsvExporter;

#[derive(Debug, serde::Serialize)]
struct CsvRecord {
    chain: String,
    address: String,
    status: String,
    balance: String,
    transaction_count: String,
    last_transaction_hash: String,
    last_transaction_time: String,
    error_message: String,
}

impl CsvExporter {
    pub fn new() -> Self {
        Self
    }

    pub fn export_to_file<P: AsRef<Path>>(
        &self,
        results: &HashMap<String, Vec<QueryResult>>,
        output_path: P,
    ) -> Result<()> {
        let file = File::create(&output_path)
            .map_err(|e| BlockchainError::IoError(e))?;
        
        let mut writer = Writer::from_writer(file);
        
        // 写入CSV头部
        writer.write_record(&[
            "Chain",
            "Address",
            "Status",
            "Balance (ETH)",
            "Transaction Count",
            "Last Transaction Hash",
            "Last Transaction Time",
            "Error Message",
        ])
        .map_err(|e| BlockchainError::IoError(e.into()))?;

        // 写入数据行
        for (chain, chain_results) in results {
            for result in chain_results {
                let record = self.query_result_to_csv_record(chain, result);
                writer.write_record(&[
                    &record.chain,
                    &record.address,
                    &record.status,
                    &record.balance,
                    &record.transaction_count,
                    &record.last_transaction_hash,
                    &record.last_transaction_time,
                    &record.error_message,
                ])
                .map_err(|e| BlockchainError::IoError(e.into()))?;
            }
        }

        writer.flush()
            .map_err(|e| BlockchainError::IoError(e.into()))?;

        Ok(())
    }

    // 删除未使用的导出为字符串方法以清除警告

    pub fn print_summary(&self, results: &HashMap<String, Vec<QueryResult>>) {
        println!("\n=== Query Results Summary ===");
        
        for (chain, chain_results) in results {
            let total = chain_results.len();
            let successful = chain_results.iter()
                .filter(|r| matches!(r.status, QueryStatus::Success))
                .count();
            let failed = total - successful;
            
            let _total_balance: u128 = 0; // QueryResult不再有balance数值字段
            
            let total_transactions: u64 = 0; // QueryResult不再有transaction_count字段

            println!("\nChain: {}", chain);
            println!("  Total addresses: {}", total);
            println!("  Successful queries: {}", successful);
            println!("  Failed queries: {}", failed);
            println!("  Total balance: {} Wei", "N/A"); // QueryResult不再有balance数值字段
            println!("  Total transactions: {}", total_transactions);
        }
    }

    fn query_result_to_csv_record(&self, chain: &str, result: &QueryResult) -> CsvRecord {
        CsvRecord {
            chain: chain.to_string(),
            address: result.address.clone(),
            status: match result.status {
                QueryStatus::Success => "Success",
                QueryStatus::Error => "Error",
                QueryStatus::NoTransactions => "No Transactions",
            }.to_string(),
            balance: result.balance.as_ref().unwrap_or(&"N/A".to_string()).clone(),
            transaction_count: result.transaction_count.to_string(),
            last_transaction_hash: result.last_transaction_hash.as_ref().unwrap_or(&"N/A".to_string()).clone(),
            last_transaction_time: result.last_transaction_time
                .as_ref()
                .map(|ts| ts.format("%Y-%m-%d %H:%M:%S UTC").to_string())
                .unwrap_or_else(|| "N/A".to_string()),
            error_message: result.error_message.clone().unwrap_or_else(|| "".to_string()),
        }
    }
}

pub struct ExcelExporter;

impl ExcelExporter {
    pub fn new() -> Self {
        Self
    }

    pub fn export_to_file<P: AsRef<Path>>(
        &self,
        results: &HashMap<String, Vec<QueryResult>>,
        output_path: P,
    ) -> Result<()> {
        let mut workbook = Workbook::new();

        // 创建标题格式
        let header_format = Format::new()
            .set_bold()
            .set_background_color(0xD9E1F2)
            .set_border(FormatBorder::Thin);

        // 为每个Chain创建一个sheet
        for (chain, chain_results) in results {
            // 清理sheet名称（Excel sheet名称有长度和字符限制）
            let sheet_name = self.sanitize_sheet_name(chain);
            
            let worksheet = workbook.add_worksheet()
                .set_name(&sheet_name)
                .map_err(|e| BlockchainError::IoError(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("Failed to set worksheet name: {}", e)
                )))?;

            // 写入表头
            let headers = [
                "Chain",
                "Address",
                "Status",
                "Balance (ETH)",
                "Transaction Count",
                "Last Transaction Hash",
                "Last Transaction Time",
                "Error Message",
            ];

            for (col, header) in headers.iter().enumerate() {
                worksheet.write_string_with_format(0, col as u16, *header, &header_format)
                    .map_err(|e| BlockchainError::IoError(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        format!("Failed to write header: {}", e)
                    )))?;
            }

            // 写入数据行
            for (row, result) in chain_results.iter().enumerate() {
                let row_num = (row + 1) as u32;
                
                // Chain
                worksheet.write_string(row_num, 0, chain)
                    .map_err(|e| BlockchainError::IoError(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        format!("Failed to write data: {}", e)
                    )))?;
                
                // Address
                worksheet.write_string(row_num, 1, &result.address)
                    .map_err(|e| BlockchainError::IoError(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        format!("Failed to write data: {}", e)
                    )))?;
                
                // Status
                let status = match result.status {
                    QueryStatus::Success => "Success",
                    QueryStatus::Error => "Error",
                    QueryStatus::NoTransactions => "No Transactions",
                };
                worksheet.write_string(row_num, 2, status)
                    .map_err(|e| BlockchainError::IoError(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        format!("Failed to write data: {}", e)
                    )))?;
                
                // Balance
                let balance_default = "N/A".to_string();
                let balance = result.balance.as_ref().unwrap_or(&balance_default);
                worksheet.write_string(row_num, 3, balance)
                    .map_err(|e| BlockchainError::IoError(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        format!("Failed to write data: {}", e)
                    )))?;
                
                // Transaction Count
                worksheet.write_number(row_num, 4, result.transaction_count as f64)
                    .map_err(|e| BlockchainError::IoError(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        format!("Failed to write data: {}", e)
                    )))?;
                
                // Last Transaction Hash
                let tx_hash_default = "N/A".to_string();
                let tx_hash = result.last_transaction_hash.as_ref().unwrap_or(&tx_hash_default);
                worksheet.write_string(row_num, 5, tx_hash)
                    .map_err(|e| BlockchainError::IoError(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        format!("Failed to write data: {}", e)
                    )))?;
                
                // Last Transaction Time
                let tx_time = result.last_transaction_time
                    .as_ref()
                    .map(|ts| ts.format("%Y-%m-%d %H:%M:%S UTC").to_string())
                    .unwrap_or_else(|| "N/A".to_string());
                worksheet.write_string(row_num, 6, &tx_time)
                    .map_err(|e| BlockchainError::IoError(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        format!("Failed to write data: {}", e)
                    )))?;
                
                // Error Message
                let error_msg_default = "".to_string();
                let error_msg = result.error_message.as_ref().unwrap_or(&error_msg_default);
                worksheet.write_string(row_num, 7, error_msg)
                    .map_err(|e| BlockchainError::IoError(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        format!("Failed to write data: {}", e)
                    )))?;
            }

            // 自动调整列宽
            for col in 0..headers.len() {
                worksheet.set_column_width(col as u16, 20.0)
                    .map_err(|e| BlockchainError::IoError(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        format!("Failed to set column width: {}", e)
                    )))?;
            }
        }

        workbook.save(output_path.as_ref())
            .map_err(|e| BlockchainError::IoError(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to save workbook: {}", e)
            )))?;

        Ok(())
    }

    /// 清理sheet名称，确保符合Excel的命名规则
    /// - 最大长度31个字符
    /// - 不能包含特殊字符: : \ / ? * [ ]
    fn sanitize_sheet_name(&self, name: &str) -> String {
        let forbidden_chars = ['/', '\\', '?', '*', '[', ']', ':'];
        let mut sanitized: String = name
            .chars()
            .filter(|c| !forbidden_chars.contains(c))
            .collect();
        
        // 限制长度为31个字符
        if sanitized.len() > 31 {
            sanitized = sanitized[..31].to_string();
        }
        
        // 如果名称为空，使用默认值
        if sanitized.is_empty() {
            sanitized = "Sheet".to_string();
        }
        
        sanitized
    }
}
