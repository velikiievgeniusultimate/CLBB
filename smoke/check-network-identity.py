#!/usr/bin/env python3
"""Regression for voice/GameSDK parsing Network.toString() as an Android netId."""
from pathlib import Path
import subprocess,tempfile
root=Path(__file__).resolve().parents[1]
with tempfile.TemporaryDirectory(prefix='clbb-network-id-') as work:
    work=Path(work)
    source=work/'Check.java'
    source.write_text('''import android.net.Network;
public class Check {public static void main(String[] args){
 Network a=new Network();if(Integer.parseInt(a.toString())!=100)throw new AssertionError();
 if(a.getNetworkHandle()!=((100L<<32)|0xcafed00dL))throw new AssertionError();
 if(!a.equals(new Network(100))||a.equals(new Network(101))||a.hashCode()!=100)throw new AssertionError();
 System.out.println("NETWORK_IDENTITY_OK numeric netId, consistent handle and equality");
}}''')
    subprocess.run(['javac','-d',str(work),str(root/'src/atl/src/api-impl/android/net/Network.java'),str(source)],check=True)
    subprocess.run(['java','-cp',str(work),'Check'],check=True)
