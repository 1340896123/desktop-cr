import React, { useEffect, useState } from 'react';
import ReactDOM from 'react-dom/client';
import { FileTransferPage } from './components/FileTransferPage';
import { getTransferDeviceName } from './services/fileTransfer';
import './styles/global.css';

/** 独立文件传输窗口入口:整窗承载文件传输页,并读取主窗口写入的对端设备名。 */
const TransferApp: React.FC = () => {
  const [deviceName, setDeviceName] = useState<string | undefined>(undefined);

  useEffect(() => {
    void getTransferDeviceName().then((name) => setDeviceName(name ?? undefined));
  }, []);

  return <FileTransferPage deviceName={deviceName} />;
};

ReactDOM.createRoot(document.getElementById('root') as HTMLElement).render(
  <React.StrictMode>
    <TransferApp />
  </React.StrictMode>,
);