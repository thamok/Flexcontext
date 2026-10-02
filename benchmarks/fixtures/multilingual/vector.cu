__device__ float scaleElement(float value, float scale) { return value * scale; }
__global__ void scaleVector(float *values, int count, float scale) {
    int index = blockIdx.x * blockDim.x + threadIdx.x;
    if (index >= count) { return; }
    values[index] = scaleElement(values[index], scale);
}
void launchScale(float *values, int count, float scale) {
    scaleVector<<<(count + 255) / 256, 256>>>(values, count, scale);
}
const char *vectorHeading() { return "Scale vector element count index launch"; }
