#!/bin/bash

# Intercom Tags Manager DMG 构建脚本
# 使用方法: ./build-dmg.sh [版本号]

set -e

# ==================== 配置 ====================
APP_NAME="Intercom Tags Manager"
EXECUTABLE_NAME="intercomtags"
BUNDLE_ID="com.intercomtags.app"
VERSION="${1:-0.1.0}"

# 路径
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
DIST_DIR="${SCRIPT_DIR}/dist"
APP_DIR="${DIST_DIR}/${APP_NAME}.app"
DMG_PATH="${DIST_DIR}/${APP_NAME}-${VERSION}.dmg"

echo "=========================================="
echo "构建 ${APP_NAME} v${VERSION}"
echo "=========================================="

# ==================== 编译 ====================
echo ""
echo "[1/5] 编译 Release 版本..."
cargo build --release

# ==================== 创建 .app 结构 ====================
echo ""
echo "[2/5] 创建 .app 结构..."
rm -rf "${DIST_DIR}"
mkdir -p "${APP_DIR}/Contents/MacOS"
mkdir -p "${APP_DIR}/Contents/Resources"

# 复制可执行文件
cp "target/release/${EXECUTABLE_NAME}" "${APP_DIR}/Contents/MacOS/${APP_NAME}"
chmod +x "${APP_DIR}/Contents/MacOS/${APP_NAME}"

# ==================== 创建 Info.plist ====================
echo ""
echo "[3/5] 创建 Info.plist..."
cat > "${APP_DIR}/Contents/Info.plist" << EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key>
    <string>zh_CN</string>
    <key>CFBundleExecutable</key>
    <string>${APP_NAME}</string>
    <key>CFBundleIconFile</key>
    <string>AppIcon</string>
    <key>CFBundleIdentifier</key>
    <string>${BUNDLE_ID}</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundleName</key>
    <string>${APP_NAME}</string>
    <key>CFBundleDisplayName</key>
    <string>${APP_NAME}</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleShortVersionString</key>
    <string>${VERSION}</string>
    <key>CFBundleVersion</key>
    <string>${VERSION}</string>
    <key>LSMinimumSystemVersion</key>
    <string>10.13</string>
    <key>NSHighResolutionCapable</key>
    <true/>
    <key>NSHumanReadableCopyright</key>
    <string>Copyright © 2024. All rights reserved.</string>
    <key>NSPrincipalClass</key>
    <string>NSApplication</string>
</dict>
</plist>
EOF

# ==================== 创建 PkgInfo ====================
echo "APPL????" > "${APP_DIR}/Contents/PkgInfo"

# ==================== 复制图标 ====================
ICON_SOURCE="${SCRIPT_DIR}/assets/AppIcon.icns"
if [ -f "${ICON_SOURCE}" ]; then
    echo ""
    echo "[3.5/5] 复制应用图标..."
    cp "${ICON_SOURCE}" "${APP_DIR}/Contents/Resources/AppIcon.icns"
else
    echo ""
    echo "⚠️ 警告: 未找到图标文件 ${ICON_SOURCE}"
    echo "   运行 ./assets/create-icon.sh 生成图标"
fi

# ==================== 创建 .dmg ====================
echo ""
echo "[4/6] 创建 .dmg..."

# 创建临时目录用于 DMG 内容
DMG_TEMP="${DIST_DIR}/dmg_temp"
mkdir -p "${DMG_TEMP}"

# 复制 .app 到临时目录
cp -R "${APP_DIR}" "${DMG_TEMP}/"

# 创建 Applications 快捷方式
ln -s /Applications "${DMG_TEMP}/Applications"

# 删除旧的 DMG
rm -f "${DMG_PATH}"

# 创建 DMG
hdiutil create -volname "${APP_NAME}" -srcfolder "${DMG_TEMP}" -ov -format UDZO "${DMG_PATH}"

# 清理临时目录
rm -rf "${DMG_TEMP}"

# ==================== 设置 DMG 图标 ====================
echo ""
echo "[5/6] 设置 DMG 图标..."

ICON_SOURCE="${SCRIPT_DIR}/assets/AppIcon.icns"
if [ -f "${ICON_SOURCE}" ]; then
    # 创建临时可读写 DMG
    TEMP_DMG="${DIST_DIR}/temp_icon.dmg"
    MOUNT_POINT="/Volumes/${APP_NAME}"
    
    # 转换为可读写格式
    hdiutil convert "${DMG_PATH}" -format UDRW -o "${TEMP_DMG}"
    
    # 挂载为可读写，并获取挂载点
    echo "正在挂载..."
    MOUNT_OUTPUT=$(hdiutil attach "${TEMP_DMG}" -nobrowse -readwrite)
    echo "挂载输出: ${MOUNT_OUTPUT}"
    
    # 等待挂载完成
    sleep 2
    
    # 从挂载输出中提取挂载路径（最后一列）
    ACTUAL_MOUNT=$(echo "${MOUNT_OUTPUT}" | grep "/Volumes/" | awk -F'\t' '{print $NF}' | tr -d '\n')
    
    if [ -n "${ACTUAL_MOUNT}" ] && [ -d "${ACTUAL_MOUNT}" ]; then
        echo "挂载路径: '${ACTUAL_MOUNT}'"
        # 复制图标到卷根目录
        cp "${ICON_SOURCE}" "${ACTUAL_MOUNT}/.VolumeIcon.icns"
        # 设置自定义图标属性
        SetFile -a C "${ACTUAL_MOUNT}"
        echo "✅ 图标文件已复制"
    else
        echo "⚠️ 无法找到挂载点，尝试查找..."
        ACTUAL_MOUNT=$(find /Volumes -maxdepth 1 -name "${APP_NAME}*" -type d | head -1)
        if [ -n "${ACTUAL_MOUNT}" ]; then
            echo "找到挂载路径: '${ACTUAL_MOUNT}'"
            cp "${ICON_SOURCE}" "${ACTUAL_MOUNT}/.VolumeIcon.icns"
            SetFile -a C "${ACTUAL_MOUNT}"
            echo "✅ 图标文件已复制"
        else
            echo "⚠️ 无法找到挂载点"
        fi
    fi
    
    # 卸载所有相关卷
    sleep 1
    find /Volumes -maxdepth 1 -name "${APP_NAME}*" -type d 2>/dev/null | while read mp; do
        echo "卸载: ${mp}"
        hdiutil detach "${mp}" -force 2>/dev/null || true
    done
    
    sleep 1
    
    # 转换回压缩格式
    rm -f "${DMG_PATH}"
    hdiutil convert "${TEMP_DMG}" -format UDZO -o "${DMG_PATH}"
    rm -f "${TEMP_DMG}"
    
    echo "✅ DMG 图标设置完成"
else
    echo "⚠️ 跳过 DMG 图标设置"
fi

# ==================== 设置 DMG 文件图标 ====================
echo ""
echo "[5.5/6] 设置 DMG 文件图标..."

# 设置自定义图标标志
SetFile -a C "${DMG_PATH}"

# 触发 Finder 刷新
touch "${DMG_PATH}"
osascript -e 'tell application "Finder" to update POSIX file "'"${DMG_PATH}"'"' 2>/dev/null || true

echo "✅ DMG 文件已标记自定义图标"
echo ""
echo "📌 提示: 如果图标未立即显示，请运行以下命令刷新 Finder:"
echo "   killall Finder"
echo "   或"
echo "   rm -rf /var/folders/*/*/com.apple.dock.iconcache; killall Dock"

# ==================== 完成 ====================
echo ""
echo "[6/6] 构建完成!"
echo "=========================================="
echo "输出文件:"
echo "  .app: ${APP_DIR}"
echo "  .dmg: ${DMG_PATH}"
echo ""
echo "文件大小: $(du -h "${DMG_PATH}" | cut -f1)"
echo "=========================================="
