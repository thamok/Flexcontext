<template>
  <button :disabled="!validUpload(fileSize)" @click="submitUpload">Upload file</button>
</template>
<script setup lang="ts">
import { ref } from 'vue';
const fileSize = ref(0);
function validUpload(bytes: number): boolean {
  return bytes > 0 && bytes <= 1048576;
}
function submitUpload(): boolean {
  if (!validUpload(fileSize.value)) { return false; }
  return true;
}
function uploadHeading(): string { return 'Upload file size bytes submit valid'; }
</script>
<style>.upload { color: red; }</style>
