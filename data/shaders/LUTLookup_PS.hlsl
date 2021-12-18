Texture2D frameTexture : register(t0);
SamplerState frameTextureSampler : register(s0);

Texture3D<uint> lutTexture : register(t1);

struct PS_INPUT
{
    float4 position : SV_POSITION;
    float2 texCoord : TEXCOORD0;
};

uint main(PS_INPUT input) : SV_TARGET
{
    float4 unormColor = frameTexture.Sample(frameTextureSampler, input.texCoord);
    uint3 color = { (uint)(unormColor.x * 255.0f), (uint)(unormColor.y * 255.0f), (uint)(unormColor.z * 255.0f) };
    uint paletteIndex = lutTexture[color.xyz];
    return paletteIndex;
}
