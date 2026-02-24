import requests
import sys

try:
    # Test direct connection to httpbin first
    print("Testing direct connection to httpbin.org...")
    direct_resp = requests.get("https://httpbin.org/get")
    print(f"Direct response status: {direct_resp.status_code}")
    print(f"Direct response length: {len(direct_resp.text)}")
    print(f"Direct response preview: {direct_resp.text[:100]}...")
    
    # Test through our proxy
    print("\nTesting through proxy...")
    proxy_resp = requests.get("http://localhost:3000/https://httpbin.org/get")
    print(f"Proxy response status: {proxy_resp.status_code}")
    print(f"Proxy response length: {len(proxy_resp.text)}")
    print(f"Proxy response: {repr(proxy_resp.text)}")
    
    # Print headers
    print(f"\nProxy response headers: {dict(proxy_resp.headers)}")
    
except Exception as e:
    print(f"Error: {e}")
    sys.exit(1)
